use crate::sync_ext::MutexExt;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MEMORY_CACHE_TTL: Duration = Duration::from_secs(5);

pub fn memory_usage_for_process_ids(pids: impl IntoIterator<Item = u32>) -> Option<u64> {
    memory_usage_for_pids(pids.into_iter().map(Pid::from_u32).collect())
}

fn memory_usage_for_pids(mut pids: Vec<Pid>) -> Option<u64> {
    if pids.is_empty() {
        return Some(0);
    }
    pids.sort_by_key(|pid| pid.as_u32());
    pids.dedup_by_key(|pid| pid.as_u32());
    let key = pids.iter().map(|pid| pid.as_u32()).collect::<Vec<_>>();
    if let Some(value) = cached_memory_usage(&key) {
        return Some(value);
    }

    let mut system = memory_system().lock_recover();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let bytes = pids
        .iter()
        .filter_map(|pid| process_memory_bytes(&system, *pid))
        .sum();
    store_memory_usage(key, bytes);
    Some(bytes)
}

fn memory_system() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| Mutex::new(System::new()))
}

#[derive(Clone)]
struct MemoryCacheEntry {
    collected_at: Instant,
    bytes: u64,
}

fn memory_cache() -> &'static Mutex<HashMap<Vec<u32>, MemoryCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<Vec<u32>, MemoryCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_memory_usage(key: &[u32]) -> Option<u64> {
    let now = Instant::now();
    let mut cache = memory_cache().lock_recover();
    cache.retain(|_, entry| now.duration_since(entry.collected_at) < MEMORY_CACHE_TTL);
    cache.get(key).map(|entry| entry.bytes)
}

fn store_memory_usage(key: Vec<u32>, bytes: u64) {
    memory_cache().lock_recover().insert(
        key,
        MemoryCacheEntry {
            collected_at: Instant::now(),
            bytes,
        },
    );
}

fn process_memory_bytes(system: &System, pid: Pid) -> Option<u64> {
    #[cfg(target_os = "macos")]
    if let Some(bytes) = macos_physical_footprint(pid) {
        return Some(bytes);
    }

    #[cfg(target_os = "linux")]
    if let Some(bytes) = linux_proportional_set_size(pid) {
        return Some(bytes);
    }

    let process = system.process(pid)?;

    #[cfg(target_os = "windows")]
    {
        let private_usage = process.virtual_memory();
        if private_usage > 0 {
            return Some(private_usage);
        }
    }

    Some(process.memory())
}

#[cfg(target_os = "macos")]
fn macos_physical_footprint(pid: Pid) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    let result = unsafe {
        libc::proc_pid_rusage(
            pid.as_u32() as libc::c_int,
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr() as *mut libc::rusage_info_t,
        )
    };
    if result == 0 {
        Some(unsafe { info.assume_init() }.ri_phys_footprint)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_proportional_set_size(pid: Pid) -> Option<u64> {
    parse_linux_pss_kib(
        &std::fs::read_to_string(format!("/proc/{}/smaps_rollup", pid.as_u32())).ok()?,
    )
    .and_then(|kib| kib.checked_mul(1024))
}

#[cfg(target_os = "linux")]
fn parse_linux_pss_kib(input: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        line.trim_start()
            .strip_prefix("Pss:")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse().ok())
    })
}
