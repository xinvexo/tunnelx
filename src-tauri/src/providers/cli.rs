use crate::error::{AppError, AppResult};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, Signal, System};

pub(crate) fn run_with_timeout(
    command: &mut Command,
    label: &str,
    timeout: Duration,
) -> AppResult<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| AppError::Msg(format!("Failed to execute {label}: {error}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map_err(Into::into);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            let detail = output_text(&output);
            return Err(AppError::Msg(if detail.is_empty() {
                format!("{label} timed out")
            } else {
                format!("{label} timed out: {detail}")
            }));
        }
        thread::sleep(Duration::from_millis(80));
    }
}

pub(crate) fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return tunnelx_watchdog_protocol::redaction::redact_sensitive_text(stdout);
    }
    tunnelx_watchdog_protocol::redaction::redact_sensitive_text(
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

/// 单个被识别出的 CLI provider 进程：pid + 它使用的配置文件路径。
#[derive(Debug, Clone)]
pub(crate) struct CliProcess {
    pub(crate) pid: u32,
    pub(crate) config_path: PathBuf,
}

/// 某个 CLI provider（ngrok / cpolar 等）的孤儿进程识别与清理参数。
/// 这些 provider 形态一致：按可执行名或命令行 stem 识别进程，从 `--config`
/// 参数（或命令行里的 .yml/.yaml）取配置路径，停止用 SIGTERM→SIGKILL 两段式。
pub(crate) struct CliProcessSpec {
    /// 日志前缀与人类可读名，如 "ngrok"。
    pub(crate) label: &'static str,
    /// 受管可执行文件名（如 paths::exe_name() 的返回）。
    pub(crate) exe_name: &'static str,
    /// 命令行里用于兜底识别的二进制 stem（不含扩展名），如 "ngrok"。
    pub(crate) binary_stem: &'static str,
    /// SIGTERM 后的优雅等待。
    pub(crate) stop_grace: Duration,
    /// SIGKILL 后的兜底等待。
    pub(crate) kill_grace: Duration,
}

impl CliProcessSpec {
    /// 清理配置路径精确等于 `config_path` 的孤儿进程，返回被处理的进程列表。
    pub(crate) fn cleanup_for_config(&self, config_path: &Path) -> AppResult<Vec<CliProcess>> {
        let target = normalize_path(config_path);
        let mut system = System::new_all();
        let targets = self
            .find_processes(&system)
            .into_iter()
            .filter(|process| normalize_path(&process.config_path) == target)
            .collect::<Vec<_>>();
        self.terminate_processes(&mut system, &targets)?;
        Ok(targets)
    }

    /// 清理配置落在 `configs_dir` 目录内的所有孤儿进程，返回被处理的进程列表。
    pub(crate) fn cleanup_under_dir(&self, configs_dir: &Path) -> AppResult<Vec<CliProcess>> {
        let configs_dir = normalize_path(configs_dir);
        let mut system = System::new_all();
        let targets = self
            .find_processes(&system)
            .into_iter()
            .filter(|process| path_is_inside(&process.config_path, &configs_dir))
            .collect::<Vec<_>>();
        self.terminate_processes(&mut system, &targets)?;
        Ok(targets)
    }

    fn find_processes(&self, system: &System) -> Vec<CliProcess> {
        let current_pid = std::process::id();
        system
            .processes()
            .iter()
            .filter_map(|(pid, process)| {
                if pid.as_u32() == current_pid {
                    return None;
                }
                if !self.looks_like_process(process.name(), process.cmd()) {
                    return None;
                }
                let config_path = config_path_from_cmd(process.cmd())?;
                Some(CliProcess {
                    pid: pid.as_u32(),
                    config_path,
                })
            })
            .collect()
    }

    fn looks_like_process(&self, name: &OsStr, cmd: &[OsString]) -> bool {
        if name.to_string_lossy().eq_ignore_ascii_case(self.exe_name) {
            return true;
        }
        cmd.iter().any(|arg| {
            Path::new(arg)
                .file_stem()
                .map(|stem| {
                    stem.to_string_lossy()
                        .eq_ignore_ascii_case(self.binary_stem)
                })
                .unwrap_or(false)
        })
    }

    fn terminate_processes(&self, system: &mut System, targets: &[CliProcess]) -> AppResult<()> {
        for target in targets {
            crate::diag::info(
                &crate::diag::provider_scope(self.label),
                format!(
                    "cleaning orphan process pid={} config={}",
                    target.pid,
                    target.config_path.display()
                ),
            );
            self.terminate_process(system, Pid::from_u32(target.pid), &target.config_path)?;
        }
        Ok(())
    }

    fn terminate_process(
        &self,
        system: &mut System,
        pid: Pid,
        config_path: &Path,
    ) -> AppResult<()> {
        let label = self.label;
        let Some(process) = system.process(pid) else {
            return Ok(());
        };
        let sent = process
            .kill_with(Signal::Term)
            .unwrap_or_else(|| process.kill());
        if !sent {
            return Err(AppError::Msg(format!(
                "failed to stop {label} process: pid={} config={}",
                pid.as_u32(),
                config_path.display()
            )));
        }
        if wait_exit(system, pid, self.stop_grace) {
            return Ok(());
        }

        let Some(process) = system.process(pid) else {
            return Ok(());
        };
        if !process.kill() {
            return Err(AppError::Msg(format!(
                "failed to kill {label} process: pid={} config={}",
                pid.as_u32(),
                config_path.display()
            )));
        }
        if wait_exit(system, pid, self.kill_grace) {
            Ok(())
        } else {
            Err(AppError::Msg(format!(
                "{label} process did not exit; stop it manually and retry: pid={} config={}",
                pid.as_u32(),
                config_path.display()
            )))
        }
    }
}

/// 从命令行解析 provider 的配置文件路径：优先 `--config`/`-config`/`-c`（含 `=` 形式），
/// 兜底取第一个 .yml/.yaml 参数。
pub(crate) fn config_path_from_cmd(cmd: &[OsString]) -> Option<PathBuf> {
    let mut it = cmd.iter();
    while let Some(arg) = it.next() {
        let text = arg.to_string_lossy();
        if text == "--config" || text == "-config" || text == "-c" {
            return it.next().map(PathBuf::from);
        }
        if let Some(path) = text
            .strip_prefix("--config=")
            .or_else(|| text.strip_prefix("-config="))
        {
            return Some(PathBuf::from(path));
        }
    }
    cmd.iter().map(PathBuf::from).find(|path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml"))
            .unwrap_or(false)
    })
}

fn wait_exit(system: &mut System, pid: Pid, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        match system.process(pid) {
            None => return true,
            Some(process) if process.status() == ProcessStatus::Zombie => return true,
            Some(_) => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// 规整路径用于比较：能 canonicalize 就用规范路径，否则退化为原路径。
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// 判断 `path` 是否落在 `root` 目录内（两侧都先规整）。
pub(crate) fn path_is_inside(path: &Path, root: &Path) -> bool {
    normalize_path(path).starts_with(normalize_path(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process::ExitStatus;

    #[test]
    fn config_path_from_cmd_reads_common_command_shapes() {
        // ngrok 风格：--config <path>
        let cmd = [
            OsString::from("/opt/tunnelx/ngrok"),
            OsString::from("start"),
            OsString::from("--all"),
            OsString::from("--config"),
            OsString::from("/tmp/tunnelx/ngrok/configs/edge.yml"),
        ];
        assert_eq!(
            config_path_from_cmd(&cmd),
            Some(PathBuf::from("/tmp/tunnelx/ngrok/configs/edge.yml"))
        );

        // ngrok 风格：--config=<path>
        let cmd = [
            OsString::from("ngrok"),
            OsString::from("start"),
            OsString::from("--config=/tmp/tunnelx/ngrok/configs/api.yaml"),
        ];
        assert_eq!(
            config_path_from_cmd(&cmd),
            Some(PathBuf::from("/tmp/tunnelx/ngrok/configs/api.yaml"))
        );

        // cpolar 风格：-config <path>
        let cmd = [
            OsString::from("/opt/tunnelx/cpolar"),
            OsString::from("start-all"),
            OsString::from("-config"),
            OsString::from("/tmp/tunnelx/cpolar/configs/edge.yml"),
        ];
        assert_eq!(
            config_path_from_cmd(&cmd),
            Some(PathBuf::from("/tmp/tunnelx/cpolar/configs/edge.yml"))
        );

        // cpolar 风格：-config=<path>
        let cmd = [
            OsString::from("cpolar"),
            OsString::from("start-all"),
            OsString::from("-config=/tmp/tunnelx/cpolar/configs/api.yaml"),
        ];
        assert_eq!(
            config_path_from_cmd(&cmd),
            Some(PathBuf::from("/tmp/tunnelx/cpolar/configs/api.yaml"))
        );

        // 兜底：命令行里第一个 .yml/.yaml
        let cmd = [
            OsString::from("cpolar"),
            OsString::from("start"),
            OsString::from("/tmp/tunnelx/cpolar/configs/fallback.yaml"),
        ];
        assert_eq!(
            config_path_from_cmd(&cmd),
            Some(PathBuf::from("/tmp/tunnelx/cpolar/configs/fallback.yaml"))
        );
    }

    #[cfg(unix)]
    fn failed_status() -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(1 << 8)
    }

    #[cfg(windows)]
    fn failed_status() -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(1)
    }

    #[test]
    fn output_text_redacts_sensitive_values() {
        let output = Output {
            status: failed_status(),
            stdout: b"failed --authtoken secret Authorization=Bearer abc".to_vec(),
            stderr: Vec::new(),
        };

        let text = output_text(&output);

        assert!(text.contains("--authtoken ***"));
        assert!(text.contains("Authorization=***"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("Bearer abc"));
    }
}
