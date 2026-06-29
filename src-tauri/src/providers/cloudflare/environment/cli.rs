use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::{CloudflareCommandOutput, CloudflaredStatus};
use crate::providers::cloudflare::paths;
use std::path::Path;
use std::process::{Command, Output};
use tauri::{AppHandle, Runtime};

pub(crate) fn resolve_cloudflared_command<R: Runtime>(app: &AppHandle<R>) -> AppResult<String> {
    let managed = paths::managed_exe(app)?;
    if managed.exists() {
        Ok(managed.to_string_lossy().to_string())
    } else {
        Err(AppError::Msg(
            "cloudflared runtime is not installed. Install it from Runtime first".into(),
        ))
    }
}

pub fn status(app: &AppHandle, _state: &crate::state::AppState) -> AppResult<CloudflaredStatus> {
    let managed_exe = paths::managed_exe(app)?;
    let managed = managed_exe.exists();
    let path = managed_exe.to_string_lossy().to_string();
    let version_output = if managed {
        Command::new(&managed_exe).arg("--version").output().ok()
    } else {
        None
    };
    let (available, version) = match version_output {
        Some(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (true, (!text.is_empty()).then_some(text))
        }
        Some(output) => {
            let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
            (false, (!text.is_empty()).then_some(text))
        }
        None => (false, None),
    };
    let managed_credentials_dir = paths::credentials_dir(app)?;
    Ok(CloudflaredStatus {
        available,
        managed,
        version,
        path,
        cert_path: None,
        home_credentials_dir: None,
        managed_credentials_dir: managed_credentials_dir.to_string_lossy().to_string(),
    })
}

fn output_to_command_output(output: Output) -> CloudflareCommandOutput {
    CloudflareCommandOutput {
        success: output.status.success(),
        stdout: crate::services::redaction::text(String::from_utf8_lossy(&output.stdout)),
        stderr: crate::services::redaction::text(String::from_utf8_lossy(&output.stderr)),
    }
}

pub(crate) fn run_cloudflared_with_cert(
    app: &AppHandle<impl Runtime>,
    args: &[&str],
    cert_file: &str,
) -> AppResult<CloudflareCommandOutput> {
    let path = resolve_cloudflared_command(app)?;
    let mut command = Command::new(&path);
    command.args(args);
    let cert_file = cert_file.trim();
    if cert_file.is_empty() {
        return Err(AppError::Msg(
            "Cloudflare authorization is required for this connection".into(),
        ));
    }
    if !Path::new(cert_file).exists() {
        return Err(AppError::Msg(
            "Cloudflare authorization credentials are missing; authorize again".into(),
        ));
    }
    command.env("TUNNEL_ORIGIN_CERT", cert_file);
    run_command(path, command)
}

fn run_command(path: String, mut command: Command) -> AppResult<CloudflareCommandOutput> {
    let output = command
        .output()
        .map_err(|e| AppError::Msg(format!("Failed to execute cloudflared ({path}): {e}")))?;
    Ok(output_to_command_output(output))
}

pub(crate) fn run_cloudflared_checked_with_cert(
    app: &AppHandle<impl Runtime>,
    args: &[&str],
    cert_file: &str,
) -> AppResult<CloudflareCommandOutput> {
    let output = run_cloudflared_with_cert(app, args, cert_file)?;
    if output.success {
        Ok(output)
    } else {
        let detail = if output.stderr.trim().is_empty() {
            output.stdout.trim()
        } else {
            output.stderr.trim()
        };
        Err(AppError::Msg(format!(
            "cloudflared command failed: {detail}"
        )))
    }
}
