use crate::sync_ext::MutexExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn watchdog_exe_slot() -> &'static Mutex<Option<PathBuf>> {
    static SLOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub(super) fn watchdog_exe_override() -> Option<PathBuf> {
    watchdog_exe_slot().lock_recover().clone()
}

pub(crate) fn set_test_watchdog_exe(path: PathBuf) {
    *watchdog_exe_slot().lock_recover() = Some(path);
}
