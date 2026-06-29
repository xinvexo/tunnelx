use crate::error::{AppError, AppResult};
use crate::providers::cli::normalize_path;
use crate::providers::contract::CLOUDFLARE_PROVIDER_ID;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, Signal, System};
use tauri::AppHandle;

use super::{CLOUDFLARED_KILL_GRACE, CLOUDFLARED_STOP_GRACE};

#[derive(Debug, Clone)]
pub(super) struct CloudflaredProcess {
    pid: u32,
    config_path: PathBuf,
}

pub(super) fn cleanup_managed_cloudflared_processes(
    app: &AppHandle<impl tauri::Runtime>,
) -> AppResult<Vec<CloudflaredProcess>> {
    let configs_dir = crate::providers::cloudflare::paths::configs_dir(app)?;
    let configs_dir = normalize_path(&configs_dir);
    let mut system = System::new_all();
    let targets = find_cloudflared_processes(&system)
        .into_iter()
        .filter(|process| normalize_path(&process.config_path).starts_with(&configs_dir))
        .collect::<Vec<_>>();
    terminate_cloudflared_processes(&mut system, &targets)?;
    Ok(targets)
}

fn terminate_cloudflared_processes(
    system: &mut System,
    targets: &[CloudflaredProcess],
) -> AppResult<()> {
    for target in targets {
        crate::diag::info(
            &crate::diag::provider_scope(CLOUDFLARE_PROVIDER_ID),
            format!(
                "cleaning orphan cloudflared process pid={} config={}",
                target.pid,
                target.config_path.display()
            ),
        );
        terminate_cloudflared_process(system, Pid::from_u32(target.pid), &target.config_path)?;
    }
    Ok(())
}

fn find_cloudflared_processes(system: &System) -> Vec<CloudflaredProcess> {
    let current_pid = std::process::id();
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if pid.as_u32() == current_pid {
                return None;
            }
            if !looks_like_cloudflared_process(process.name(), process.cmd()) {
                return None;
            }
            let config_path = cloudflared_config_path_from_cmd(process.cmd())?;
            Some(CloudflaredProcess {
                pid: pid.as_u32(),
                config_path,
            })
        })
        .collect()
}

fn looks_like_cloudflared_process(name: &OsStr, cmd: &[OsString]) -> bool {
    if name.to_string_lossy().eq_ignore_ascii_case("cloudflared") {
        return true;
    }
    cmd.iter().any(|arg| {
        Path::new(arg)
            .file_stem()
            .map(|stem| stem.to_string_lossy().eq_ignore_ascii_case("cloudflared"))
            .unwrap_or(false)
    })
}

fn cloudflared_config_path_from_cmd(cmd: &[OsString]) -> Option<PathBuf> {
    let mut it = cmd.iter();
    while let Some(arg) = it.next() {
        let text = arg.to_string_lossy();
        if text == "--config" || text == "-c" {
            return it.next().map(PathBuf::from);
        }
        if let Some(path) = text.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn terminate_cloudflared_process(
    system: &mut System,
    pid: Pid,
    config_path: &Path,
) -> AppResult<()> {
    let Some(process) = system.process(pid) else {
        return Ok(());
    };
    let sent = process
        .kill_with(Signal::Term)
        .unwrap_or_else(|| process.kill());
    if !sent {
        return Err(AppError::Msg(format!(
            "failed to stop cloudflared process: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )));
    }
    if wait_cloudflared_exit(system, pid, CLOUDFLARED_STOP_GRACE) {
        return Ok(());
    }

    let Some(process) = system.process(pid) else {
        return Ok(());
    };
    if !process.kill() {
        return Err(AppError::Msg(format!(
            "failed to kill cloudflared process: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )));
    }
    if wait_cloudflared_exit(system, pid, CLOUDFLARED_KILL_GRACE) {
        Ok(())
    } else {
        Err(AppError::Msg(format!(
            "cloudflared process did not exit; stop it manually and retry: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )))
    }
}

fn wait_cloudflared_exit(system: &mut System, pid: Pid, timeout: Duration) -> bool {
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
        std::thread::sleep(Duration::from_millis(50));
    }
}
