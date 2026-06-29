use crate::error::{AppError, AppResult};
use crate::providers::cli::{normalize_path, path_is_inside};
use crate::providers::contract::FRP_PROVIDER_ID;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, Signal, System};

const ORPHAN_FRPC_STOP_GRACE: Duration = Duration::from_secs(2);
const ORPHAN_FRPC_KILL_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct FrpcProcess {
    pub pid: u32,
    pub config_path: PathBuf,
}

#[derive(Debug, Default, Clone)]
pub struct FrpcProcessProtection {
    pids: HashSet<u32>,
    config_paths: HashSet<PathBuf>,
}

impl FrpcProcessProtection {
    pub fn protect_pid(&mut self, pid: u32) {
        self.pids.insert(pid);
    }

    pub fn protect_config_path(&mut self, path: impl AsRef<Path>) {
        self.config_paths.insert(normalize_path(path.as_ref()));
    }

    fn protects(&self, process: &FrpcProcess) -> bool {
        self.pids.contains(&process.pid)
            || self
                .config_paths
                .contains(&normalize_path(&process.config_path))
    }
}

pub fn cleanup_orphan_frpc_in_runtime_dir(
    runtime_dir: &Path,
    protection: &FrpcProcessProtection,
) -> AppResult<Vec<FrpcProcess>> {
    let runtime_dir = normalize_path(runtime_dir);
    let mut system = System::new_all();
    let targets = find_frpc_processes(&system)
        .into_iter()
        .filter(|process| path_is_inside(&process.config_path, &runtime_dir))
        .filter(|process| !protection.protects(process))
        .collect::<Vec<_>>();

    terminate_processes(&mut system, &targets)?;
    Ok(targets)
}

pub fn cleanup_frpc_for_config(config_path: &Path) -> AppResult<Vec<FrpcProcess>> {
    let target = normalize_path(config_path);
    let mut system = System::new_all();
    let targets = find_frpc_processes(&system)
        .into_iter()
        .filter(|process| normalize_path(&process.config_path) == target)
        .collect::<Vec<_>>();

    terminate_processes(&mut system, &targets)?;
    Ok(targets)
}

pub fn cleanup_all_frpc_in_runtime_dir(runtime_dir: &Path) -> AppResult<Vec<FrpcProcess>> {
    cleanup_orphan_frpc_in_runtime_dir(runtime_dir, &FrpcProcessProtection::default())
}

fn terminate_processes(system: &mut System, targets: &[FrpcProcess]) -> AppResult<()> {
    for target in targets {
        crate::diag::info(
            &crate::diag::provider_scope(FRP_PROVIDER_ID),
            format!(
                "cleaning orphan frpc process pid={} config={}",
                target.pid,
                target.config_path.display()
            ),
        );
        terminate_process(system, Pid::from_u32(target.pid), &target.config_path)?;
    }
    Ok(())
}

fn find_frpc_processes(system: &System) -> Vec<FrpcProcess> {
    let current_pid = std::process::id();
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if pid.as_u32() == current_pid {
                return None;
            }
            if !looks_like_frpc_process(process.name(), process.cmd()) {
                return None;
            }
            let config_path = config_path_from_cmd(process.cmd())?;
            Some(FrpcProcess {
                pid: pid.as_u32(),
                config_path,
            })
        })
        .collect()
}

fn looks_like_frpc_process(name: &OsStr, cmd: &[OsString]) -> bool {
    if name.to_string_lossy().eq_ignore_ascii_case("frpc") {
        return true;
    }
    cmd.iter().any(|arg| {
        Path::new(arg)
            .file_stem()
            .map(|stem| stem.to_string_lossy().eq_ignore_ascii_case("frpc"))
            .unwrap_or(false)
    })
}

fn config_path_from_cmd(cmd: &[OsString]) -> Option<PathBuf> {
    let mut it = cmd.iter();
    while let Some(arg) = it.next() {
        let text = arg.to_string_lossy();
        if text == "-c" || text == "--config" {
            return it.next().map(PathBuf::from);
        }
        if let Some(path) = text.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }

    cmd.iter()
        .map(PathBuf::from)
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
}

fn terminate_process(system: &mut System, pid: Pid, config_path: &Path) -> AppResult<()> {
    let Some(process) = system.process(pid) else {
        return Ok(());
    };

    let sent = process
        .kill_with(Signal::Term)
        .unwrap_or_else(|| process.kill());
    if !sent {
        return Err(AppError::Msg(format!(
            "Found a leftover frpc process still using this config, but failed to send a stop signal: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )));
    }

    if wait_process_exit(system, pid, ORPHAN_FRPC_STOP_GRACE) {
        return Ok(());
    }

    let Some(process) = system.process(pid) else {
        return Ok(());
    };
    if !process.kill() {
        return Err(AppError::Msg(format!(
            "Found a leftover frpc process still using this config, but failed to force kill it: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )));
    }
    if wait_process_exit(system, pid, ORPHAN_FRPC_KILL_GRACE) {
        Ok(())
    } else {
        Err(AppError::Msg(format!(
            "Leftover frpc process did not exit; stop it manually and retry: pid={} config={}",
            pid.as_u32(),
            config_path.display()
        )))
    }
}

fn wait_process_exit(system: &mut System, pid: Pid, timeout: Duration) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use std::process::{Child, Command, Stdio};

    fn unique_test_dir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let path = std::env::temp_dir()
            .join("tunnelx-tests")
            .join(name)
            .join(suffix);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(unix)]
    fn write_executable_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    fn spawn_fake_frpc(exe: &Path, config_path: &Path) -> Child {
        Command::new(exe)
            .arg("-c")
            .arg(config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[cfg(unix)]
    fn fake_frpc(root: &Path) -> PathBuf {
        let exe = root.join("frpc");
        write_executable_script(
            &exe,
            r#"#!/bin/sh
    trap 'exit 0' TERM
    while true; do
      sleep 1
    done
    "#,
        );
        exe
    }

    fn wait_child_exit(child: &mut Child) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            assert!(Instant::now() < deadline, "fake frpc did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn config_path_from_cmd_accepts_common_shapes() {
        assert_eq!(
            config_path_from_cmd(&[
                OsString::from("frpc"),
                OsString::from("-c"),
                OsString::from("/tmp/a.json")
            ]),
            Some(PathBuf::from("/tmp/a.json"))
        );
        assert_eq!(
            config_path_from_cmd(&[
                OsString::from("frpc"),
                OsString::from("--config=/tmp/b.json")
            ]),
            Some(PathBuf::from("/tmp/b.json"))
        );
    }

    #[test]
    #[cfg(unix)]
    fn cleanup_config_stops_matching_process() {
        let root = unique_test_dir("lifecycle-config");
        let exe = fake_frpc(&root);
        let cfg_path = root.join("profile.json");
        fs::write(&cfg_path, "{}").unwrap();
        let mut child = spawn_fake_frpc(&exe, &cfg_path);

        std::thread::sleep(Duration::from_millis(120));
        let killed = cleanup_frpc_for_config(&cfg_path).unwrap();

        assert!(killed.iter().any(|process| process.pid == child.id()));
        wait_child_exit(&mut child);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn cleanup_runtime_dir_stops_unprotected_process() {
        let root = unique_test_dir("lifecycle-runtime");
        let exe = fake_frpc(&root);
        let runtime_dir = root.join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let cfg_path = runtime_dir.join("old-profile.json");
        fs::write(&cfg_path, "{}").unwrap();
        let mut child = spawn_fake_frpc(&exe, &cfg_path);

        std::thread::sleep(Duration::from_millis(120));
        let killed =
            cleanup_orphan_frpc_in_runtime_dir(&runtime_dir, &FrpcProcessProtection::default())
                .unwrap();

        assert!(killed.iter().any(|process| process.pid == child.id()));
        wait_child_exit(&mut child);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn cleanup_runtime_dir_keeps_protected_config() {
        let root = unique_test_dir("lifecycle-protected");
        let exe = fake_frpc(&root);
        let runtime_dir = root.join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let cfg_path = runtime_dir.join("active-profile.json");
        fs::write(&cfg_path, "{}").unwrap();
        let mut child = spawn_fake_frpc(&exe, &cfg_path);

        std::thread::sleep(Duration::from_millis(120));
        let mut protection = FrpcProcessProtection::default();
        protection.protect_config_path(&cfg_path);
        let killed = cleanup_orphan_frpc_in_runtime_dir(&runtime_dir, &protection).unwrap();

        assert!(!killed.iter().any(|process| process.pid == child.id()));
        assert!(child.try_wait().unwrap().is_none());
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(root);
    }
}
