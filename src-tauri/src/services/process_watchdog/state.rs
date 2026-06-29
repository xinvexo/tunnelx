use crate::sync_ext::MutexExt;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::Write;
use std::net::TcpStream;
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use tunnelx_watchdog_protocol::WatchdogResponse;

pub(super) struct WatchdogHandle {
    pub(super) pid: u32,
    pub(super) child: Option<Child>,
    pub(super) writer: WatchdogWriter,
}

pub(super) enum WatchdogWriter {
    Stdin(ChildStdin),
    Tcp(TcpStream),
}

impl WatchdogWriter {
    pub(super) fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        match self {
            WatchdogWriter::Stdin(stdin) => {
                stdin.write_all(line.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()
            }
            WatchdogWriter::Tcp(stream) => {
                stream.write_all(line.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()
            }
        }
    }
}

#[derive(Default)]
pub(super) struct ProcessWatchdogInner {
    pub(super) handle: Option<WatchdogHandle>,
    pub(super) generation: u64,
    pub(super) next_seq: u64,
    pub(super) pending: HashMap<String, Sender<WatchdogResponse>>,
    pub(super) logs: VecDeque<String>,
}

#[derive(Clone, Default)]
pub struct ProcessWatchdogState(Arc<Mutex<ProcessWatchdogInner>>);

impl ProcessWatchdogState {
    pub(super) fn lock(&self) -> MutexGuard<'_, ProcessWatchdogInner> {
        self.0.lock_recover()
    }

    pub fn pid(&self) -> Option<u32> {
        self.lock().handle.as_ref().map(|handle| handle.pid)
    }

    pub(super) fn clear(&self) {
        let mut inner = self.lock();
        inner.handle = None;
        inner.pending.clear();
    }

    pub(super) fn clear_if_generation(&self, generation: u64) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.handle = None;
        inner.pending.clear();
        true
    }

    pub(super) fn is_generation(&self, generation: u64) -> bool {
        self.lock().generation == generation
    }

    pub(super) fn take_handle(&self) -> Option<WatchdogHandle> {
        let mut inner = self.lock();
        inner.pending.clear();
        inner.handle.take()
    }

    pub(super) fn next_request(&self) -> (String, u64, Receiver<WatchdogResponse>) {
        let mut inner = self.lock();
        inner.next_seq = inner.next_seq.saturating_add(1);
        let seq = inner.next_seq;
        let id = format!("req-{seq}");
        let (tx, rx) = mpsc::channel();
        inner.pending.insert(id.clone(), tx);
        (id, seq, rx)
    }

    pub(super) fn remove_pending(&self, id: &str) {
        self.lock().pending.remove(id);
    }

    pub(super) fn complete_response(&self, id: &str, response: WatchdogResponse) -> bool {
        self.lock()
            .pending
            .remove(id)
            .map(|sender| sender.send(response).is_ok())
            .unwrap_or(false)
    }

    pub(crate) fn push_log(&self, line: String) {
        const LOG_LIMIT: usize = 1000;
        let mut inner = self.lock();
        while inner.logs.len() >= LOG_LIMIT {
            inner.logs.pop_front();
        }
        inner.logs.push_back(line);
    }

    pub(crate) fn logs(&self) -> Vec<String> {
        self.lock().logs.iter().cloned().collect()
    }

    pub(super) fn alive_locked(inner: &mut ProcessWatchdogInner) -> bool {
        match inner.handle.as_mut() {
            Some(handle) => {
                if let Some(child) = handle.child.as_mut() {
                    match child.try_wait() {
                        Ok(None) => true,
                        _ => {
                            inner.handle = None;
                            false
                        }
                    }
                } else {
                    true
                }
            }
            None => false,
        }
    }

    pub(super) fn send_locked(inner: &mut ProcessWatchdogInner, line: &str) -> bool {
        if let Some(handle) = inner.handle.as_mut() {
            let ok = handle.writer.write_line(line).is_ok();
            if ok {
                return true;
            }
            inner.handle = None;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_event_reader_does_not_clear_new_generation() {
        let state = ProcessWatchdogState::default();
        let (id, _, _) = state.next_request();
        {
            let mut inner = state.lock();
            inner.generation = 2;
        }

        assert!(!state.clear_if_generation(1));
        assert!(state.lock().pending.contains_key(&id));

        assert!(state.clear_if_generation(2));
        assert!(state.lock().pending.is_empty());
    }

    #[test]
    fn generation_check_identifies_current_watchdog() {
        let state = ProcessWatchdogState::default();
        {
            let mut inner = state.lock();
            inner.generation = 7;
        }

        assert!(state.is_generation(7));
        assert!(!state.is_generation(6));
    }
}
