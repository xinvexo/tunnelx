use crate::cleanup::{cleanup_plan, execute_http_request};
use crate::emitter::Emitter;
use crate::platform;
use crate::supervisor::SupervisorMessage;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use tunnelx_watchdog_protocol::WatchdogEnvVar;
use tunnelx_watchdog_protocol::{WatchdogCleanupPlan, WatchdogEvent, WatchdogStopStrategy};

const ADMIN_GRACE: Duration = Duration::from_secs(2);
const RECENT_LOG_LIMIT: usize = 80;
const RECENT_LOG_LINE_LIMIT: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProcessKey {
    pub(crate) provider_id: String,
    pub(crate) tunnel_id: String,
}

impl ProcessKey {
    pub(crate) fn new(provider_id: String, tunnel_id: String) -> Self {
        Self {
            provider_id,
            tunnel_id,
        }
    }
}

pub(crate) struct ManagedProcess {
    pub(crate) key: ProcessKey,
    pub(crate) args: Vec<String>,
    recent_logs: VecDeque<String>,
    child: Child,
    stop_strategy: Option<Box<WatchdogStopStrategy>>,
    cleanup: Option<Box<WatchdogCleanupPlan>>,
}

pub(crate) struct TerminateOutcome {
    pub(crate) forced: bool,
    pub(crate) cleanup: Result<(), String>,
}

impl ManagedProcess {
    pub(crate) fn new(
        key: ProcessKey,
        args: Vec<String>,
        child: Child,
        stop_strategy: Option<Box<WatchdogStopStrategy>>,
        cleanup: Option<Box<WatchdogCleanupPlan>>,
    ) -> Self {
        Self {
            key,
            args,
            recent_logs: VecDeque::new(),
            child,
            stop_strategy,
            cleanup,
        }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn push_recent_log(&mut self, line: String) {
        while self.recent_logs.len() >= RECENT_LOG_LIMIT {
            self.recent_logs.pop_front();
        }
        self.recent_logs.push_back(bounded_recent_log(line));
    }

    pub(crate) fn recent_logs(&self) -> Vec<String> {
        self.recent_logs.iter().cloned().collect()
    }
}

fn bounded_recent_log(line: String) -> String {
    if line.chars().count() <= RECENT_LOG_LINE_LIMIT {
        return line;
    }
    let mut bounded = line.chars().take(RECENT_LOG_LINE_LIMIT).collect::<String>();
    bounded.push_str("...");
    bounded
}

pub(crate) fn spawn_managed_process(
    program: &str,
    args: &[String],
    env: &[WatchdogEnvVar],
) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for item in env {
        if !item.name.trim().is_empty() {
            command.env(item.name.trim(), item.value.as_str());
        }
    }
    platform::configure_command(&mut command);
    command.spawn()
}

pub(crate) fn spawn_log_reader<R: Read + Send + 'static>(
    emitter: Emitter,
    key: ProcessKey,
    reader: R,
    supervisor: Sender<SupervisorMessage>,
) {
    thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            emitter.send_event(WatchdogEvent::ProcessLog {
                provider_id: key.provider_id.clone(),
                tunnel_id: key.tunnel_id.clone(),
                line: line.clone(),
            });
            let _ = supervisor.send(SupervisorMessage::ProcessLog {
                key: key.clone(),
                line,
            });
        }
    });
}

pub(crate) fn reap_exited(children: &mut HashMap<ProcessKey, ManagedProcess>, emitter: &Emitter) {
    let mut finished: Vec<(ProcessKey, bool, Option<i32>)> = Vec::new();
    for (key, process) in children.iter_mut() {
        if let Ok(Some(status)) = process.child.try_wait() {
            finished.push((key.clone(), status.success(), exit_code(status)));
        }
    }
    for (key, success, code) in finished {
        if let Some(process) = children.remove(&key) {
            let cleanup = cleanup_plan(process.cleanup.as_deref());
            emitter.send_event(WatchdogEvent::ProcessExit {
                provider_id: process.key.provider_id,
                tunnel_id: process.key.tunnel_id,
                success,
                forced: false,
                code,
                cleanup_success: Some(cleanup.is_ok()),
                cleanup_error: cleanup.err(),
            });
        }
    }
}

pub(crate) fn shutdown_all(children: &mut HashMap<ProcessKey, ManagedProcess>, emitter: &Emitter) {
    let handles: Vec<_> = children
        .drain()
        .map(|(_, mut process)| {
            let emitter = emitter.clone();
            thread::spawn(move || {
                let outcome = terminate_and_cleanup(&mut process);
                emitter.send_event(WatchdogEvent::ProcessExit {
                    provider_id: process.key.provider_id,
                    tunnel_id: process.key.tunnel_id,
                    success: true,
                    code: Some(0),
                    forced: outcome.forced,
                    cleanup_success: Some(outcome.cleanup.is_ok()),
                    cleanup_error: outcome.cleanup.err(),
                });
            })
        })
        .collect();
    for handle in handles {
        let _ = handle.join();
    }
}

pub(crate) fn terminate_and_cleanup(process: &mut ManagedProcess) -> TerminateOutcome {
    let forced = if try_graceful_stop(process) {
        false
    } else {
        platform::force_kill(&mut process.child);
        true
    };
    TerminateOutcome {
        forced,
        cleanup: cleanup_plan(process.cleanup.as_deref()),
    }
}

fn try_graceful_stop(process: &mut ManagedProcess) -> bool {
    match process.stop_strategy.as_deref() {
        Some(WatchdogStopStrategy::Http { request, grace_ms }) => {
            let accepted = execute_http_request(request).is_ok();
            let grace = grace_ms.map(Duration::from_millis).unwrap_or(ADMIN_GRACE);
            if accepted && platform::wait_for(&mut process.child, grace) {
                let _ = process.child.wait();
                eprintln!(
                    "[tunnelx-watchdog] gracefully stopped {}:{}",
                    process.key.provider_id, process.key.tunnel_id
                );
                return true;
            }
            eprintln!("[tunnelx-watchdog] graceful stop did not complete (accepted={accepted}); falling back to force kill");
            false
        }
        None => false,
    }
}

fn exit_code(status: ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(code) = status.code() {
            return Some(code);
        }
        status.signal().map(|signal| 128 + signal)
    }
    #[cfg(windows)]
    {
        status.code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_log_keeps_short_lines() {
        assert_eq!(bounded_recent_log("ready".into()), "ready");
    }

    #[test]
    fn recent_log_bounds_long_lines() {
        let bounded = bounded_recent_log("x".repeat(RECENT_LOG_LINE_LIMIT + 10));

        assert_eq!(bounded.chars().count(), RECENT_LOG_LINE_LIMIT + 3);
        assert!(bounded.ends_with("..."));
    }
}
