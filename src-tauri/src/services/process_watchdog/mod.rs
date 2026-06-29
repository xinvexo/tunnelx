mod binary;
mod client;
mod events;
mod state;

pub use client::{
    ensure, recover_if_available, request, request_if_alive, send, send_if_alive, shutdown,
};
pub use state::ProcessWatchdogState;

#[cfg(test)]
pub(crate) use binary::set_test_watchdog_exe;
