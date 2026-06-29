use crate::error::{AppError, AppResult};
use crate::providers::contract::emit_runtime_install_progress;
use futures_util::StreamExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const UA: &str = "tunnelx";

pub(crate) async fn download_with_progress(
    app: &AppHandle,
    provider_id: &str,
    runtime: &str,
    url: &str,
    dest: &Path,
    max_bytes: u64,
) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut received = 0_u64;
    let mut last_emitted = 0_u64;
    emit_runtime_install_progress(app, provider_id, runtime, 0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        received += chunk.len() as u64;
        if received > max_bytes {
            return Err(AppError::Msg(format!(
                "{runtime} download is larger than expected"
            )));
        }
        file.write_all(&chunk).await?;
        if total == Some(received) || received.saturating_sub(last_emitted) >= 256 * 1024 {
            emit_runtime_install_progress(app, provider_id, runtime, received, total);
            last_emitted = received;
        }
    }
    file.flush().await?;
    emit_runtime_install_progress(app, provider_id, runtime, received, total);
    Ok(())
}

pub(crate) fn temp_file(bin_dir: &Path, exe_name: &str, label: &str) -> PathBuf {
    bin_dir.join(format!(".{exe_name}-{}-{label}.tmp", Uuid::new_v4()))
}

#[cfg(unix)]
pub(crate) fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    fs::rename(temp, target)?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    let backup = target.with_file_name(format!(
        ".{}-{}.backup",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runtime"),
        Uuid::new_v4()
    ));
    let had_target = target.exists();
    if had_target {
        fs::rename(target, &backup)?;
    }
    match fs::rename(temp, target) {
        Ok(()) => {
            if had_target {
                let _ = fs::remove_file(&backup);
            }
            Ok(())
        }
        Err(error) => {
            if had_target {
                let _ = fs::rename(&backup, target);
            }
            Err(error.into())
        }
    }
}

#[cfg(unix)]
pub(crate) fn set_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path) -> AppResult<()> {
    Ok(())
}

pub(crate) fn command_output_text(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

pub(crate) fn validate_executable(command: &mut Command, runtime: &str) -> AppResult<()> {
    let output = command
        .output()
        .map_err(|error| AppError::Msg(format!("Failed to execute {runtime}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = command_output_text(&output);
    Err(AppError::Msg(if detail.is_empty() {
        format!("{runtime} executable validation failed")
    } else {
        format!("{runtime} executable validation failed: {detail}")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_executable_accepts_successful_version_command() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.arg("--help");

        validate_executable(&mut command, "test-runtime").unwrap();
    }

    #[test]
    fn validate_executable_rejects_failed_version_command() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.arg("--definitely-not-a-libtest-flag");

        assert!(validate_executable(&mut command, "test-runtime").is_err());
    }
}
