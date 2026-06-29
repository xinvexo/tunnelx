use crate::sync_ext::MutexExt;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
pub struct SessionInner {
    pub loaded: bool,
    pub save_blocked: bool,
}

#[derive(Clone, Default)]
pub struct SessionState(Arc<Mutex<SessionInner>>);

impl SessionState {
    pub fn lock(&self) -> MutexGuard<'_, SessionInner> {
        self.0.lock_recover()
    }
}
