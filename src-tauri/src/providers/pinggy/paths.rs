use crate::error::AppResult;
use std::path::PathBuf;
use tauri::{AppHandle, Runtime};

pub(crate) fn sanitize_filename(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        "pinggy".into()
    } else {
        trimmed
    }
}

pub fn root<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(crate::paths::data_dir(app)?.join("pinggy"))
}

pub fn configs_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("configs"))
}

pub fn bin_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("bin"))
}

pub fn home_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("home"))
}

pub fn state_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("state"))
}

pub fn config_home_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("config-home"))
}

pub fn exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pinggy.exe"
    } else {
        "pinggy"
    }
}

pub fn managed_exe<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(bin_dir(app)?.join(exe_name()))
}

pub fn config_file<R: Runtime>(app: &AppHandle<R>, connection_id: &str) -> AppResult<PathBuf> {
    Ok(configs_dir(app)?.join(format!("{}.json", sanitize_filename(connection_id))))
}
