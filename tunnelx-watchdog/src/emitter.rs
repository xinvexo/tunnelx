use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tunnelx_watchdog_protocol::{WatchdogEvent, WatchdogFrame, WatchdogResponse};

trait EventWriter: Write + Send {}
impl<T: Write + Send> EventWriter for T {}

#[derive(Clone)]
pub(crate) struct Emitter {
    out: Arc<Mutex<Box<dyn EventWriter>>>,
    seq: Arc<AtomicU64>,
}

impl Emitter {
    pub(crate) fn new() -> Self {
        Self {
            out: Arc::new(Mutex::new(Box::new(std::io::stdout()))),
            seq: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(crate) fn attach_stream(&self, stream: TcpStream) {
        if let Ok(mut out) = self.out.lock() {
            *out = Box::new(stream);
        }
    }

    fn send(&self, json: String) {
        if let Ok(mut out) = self.out.lock() {
            let _ = out.write_all(json.as_bytes());
            let _ = out.write_all(b"\n");
            let _ = out.flush();
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    pub(crate) fn send_response(&self, id: impl Into<String>, response: WatchdogResponse) {
        let seq = self.next_seq();
        match serde_json::to_string(&WatchdogFrame::response(id, seq, response)) {
            Ok(json) => self.send(json),
            Err(error) => eprintln!("[tunnelx-watchdog] failed to serialize response: {error}"),
        }
    }

    pub(crate) fn send_event(&self, event: WatchdogEvent) {
        let seq = self.next_seq();
        match serde_json::to_string(&WatchdogFrame::event(format!("event-{seq}"), seq, event)) {
            Ok(json) => self.send(json),
            Err(error) => eprintln!("[tunnelx-watchdog] failed to serialize event: {error}"),
        }
    }
}
