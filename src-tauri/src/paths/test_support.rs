use crate::sync_ext::MutexExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn data_dir_slot() -> &'static Mutex<Option<PathBuf>> {
    static SLOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub(super) fn data_dir_override() -> Option<PathBuf> {
    data_dir_slot().lock_recover().clone()
}

pub(crate) fn set_test_data_dir(path: PathBuf) {
    *data_dir_slot().lock_recover() = Some(path);
}

/// 串行化所有依赖进程级测试数据目录覆盖的测试：并行运行时它们会互相覆盖
/// 同一个全局 `data_dir_slot`，导致路径错乱。用法是在测试开头持有该守卫：
/// `let _guard = crate::paths::data_dir_test_guard();`（直到测试结束才释放）。
pub(crate) fn data_dir_test_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(())).lock_recover()
}
