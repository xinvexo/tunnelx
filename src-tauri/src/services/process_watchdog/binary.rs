use crate::error::{AppError, AppResult};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
mod test_support;
use tauri::{AppHandle, Manager, Runtime};
#[cfg(test)]
pub(crate) use test_support::set_test_watchdog_exe;

pub(super) fn watchdog_exe<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_support::watchdog_exe_override() {
        return Ok(path);
    }

    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe()?.parent().map(Path::to_path_buf) {
        collect_watchdog_candidates(&dir, &mut candidates);
    }
    if let Ok(dir) = app.path().resource_dir() {
        collect_watchdog_candidates(&dir, &mut candidates);
    }

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    let searched = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(AppError::Msg(format!(
        "watchdog executable was not found. Run `pnpm build:watchdog` first. Checked: {searched}"
    )))
}

fn collect_watchdog_candidates(dir: &Path, out: &mut Vec<PathBuf>) {
    push_unique_path(
        out,
        dir.join(format!("tunnelx-watchdog{}", std::env::consts::EXE_SUFFIX)),
    );

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("tunnelx-watchdog") {
            push_unique_path(out, path);
        }
    }
}

fn push_unique_path(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|existing| existing == &path) {
        out.push(path);
    }
}
