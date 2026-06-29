//! frpc 二进制版本管理：查询 GitHub releases、按平台下载、解压、激活、删除。
use crate::error::{AppError, AppResult};
use crate::providers::contract::{ProviderRuntimeUpdateStatus, FRP_PROVIDER_ID};
use crate::providers::frp::paths;
use crate::providers::frp::services::frpc_service;
use crate::providers::frp::state::frp_state;
use crate::state::AppState;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const RELEASES_API: &str = "https://api.github.com/repos/fatedier/frp/releases";
const UA: &str = "tunnelx";

/// 临时文件/目录名的进程内单调计数器，保证并发操作各自的临时路径唯一。
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 清扫 frpc_root 下陈旧的 `.download-*` 临时文件（上次安装中途被杀残留的）。
/// 只删修改时间超过 1 小时的，避免误删另一个正在进行的并发下载。
/// 尽力而为，错误忽略。在每次新安装前调用，避免临时文件无限累积。
async fn sweep_stale_downloads(root: &Path) {
    const STALE_AFTER: Duration = Duration::from_secs(3600);
    let Ok(mut rd) = tokio::fs::read_dir(root).await else {
        return;
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".download-")
        {
            continue;
        }
        let stale = entry
            .metadata()
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|age| age >= STALE_AFTER)
            .unwrap_or(false);
        if stale {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

fn save_active_frpc_version(
    app: &AppHandle,
    state: &AppState,
    version: Option<String>,
) -> AppResult<()> {
    let before = state.config.snapshot()?;
    state.config.set_active_frpc_version(version)?;
    if let Err(error) = crate::store::save_app_data(app, state) {
        state.config.replace(before);
        return Err(error);
    }
    Ok(())
}

/// 给 GitHub 下载链接套上镜像前缀（ghproxy 风格：`<前缀>/<原始 github url>`）。
/// 前缀为空时返回原链接（直连 GitHub）。只用于二进制大文件下载，
/// releases API（api.github.com 的小 JSON）不走镜像，ghproxy 类站点也不代理它。
///
/// 防御性校验：非 https 前缀一律忽略（退回直连），避免明文 http 镜像被中间人投毒。
/// 设置入口（config_state）已拒绝非 https，这里是第二道防线。
fn apply_mirror(mirror: &str, url: &str) -> String {
    let mirror = mirror.trim();
    if mirror.is_empty() || !mirror.starts_with("https://") {
        return url.to_string();
    }
    format!("{}/{}", mirror.trim_end_matches('/'), url)
}

/// 返回给前端的版本条目（远端可下载 + 本地已安装的并集）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersion {
    pub version: String,
    pub tag: String,
    pub published_at: String,
    pub asset_name: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub installed: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionList {
    pub versions: Vec<RemoteVersion>,
    pub remote_error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    version: String,
    received: u64,
    total: u64,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// 当前平台对应 frp 资源名里的 `_<os>_<arch>` 片段。
fn platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "386"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        "amd64"
    };
    (os, arch)
}

fn asset_matches(name: &str, os: &str, arch: &str) -> bool {
    (name.ends_with(".tar.gz") || name.ends_with(".zip"))
        && name.contains(&format!("_{os}_{arch}."))
}

fn parse_ver(v: &str) -> (u32, u32, u32) {
    let mut it = v.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0)
    });
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// frp 自 v0.52.0 起支持新的 TOML/YAML/JSON 配置格式；更老版本不展示。
fn supports_json(v: &str) -> bool {
    let (major, minor, _) = parse_ver(v);
    major > 0 || minor >= 52
}

fn installed_versions(app: &AppHandle) -> AppResult<Vec<String>> {
    let root = paths::frpc_root(app)?;
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&root) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // 单个目录名异常只跳过该项，不能让整个版本列表失败。
            match paths::frpc_exe(app, &name) {
                Ok(exe) if exe.exists() => out.push(name),
                _ => {}
            }
        }
    }
    Ok(out)
}

async fn fetch_remote() -> AppResult<Vec<GhRelease>> {
    // 列表是小响应，给总超时即可；不带超时的话 GitHub 连接被黑洞会让整个版本页挂死。
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?;
    let releases = client
        .get(RELEASES_API)
        .query(&[("per_page", "30")])
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<GhRelease>>()
        .await?;
    Ok(releases)
}

/// 列出版本：远端 releases ∪ 本地已安装。远端不可达时只返回本地，并把错误带给 UI。
pub async fn list(app: AppHandle, state: AppState) -> AppResult<VersionList> {
    let installed = installed_versions(&app)?;
    let settings = state.config.frp_settings();
    let active = settings.active_frpc_version;
    let mirror = settings.frpc_mirror.unwrap_or_default();
    let (os, arch) = platform();
    let remote = fetch_remote().await;
    let remote_error = remote.as_ref().err().map(ToString::to_string);

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(releases) = remote {
        for r in releases {
            if r.prerelease {
                continue;
            }
            let version = r.tag_name.trim_start_matches('v').to_string();
            if !supports_json(&version) {
                continue;
            }
            let asset = r
                .assets
                .into_iter()
                .find(|a| asset_matches(&a.name, os, arch));
            seen.insert(version.clone());
            out.push(RemoteVersion {
                installed: installed.contains(&version),
                active: active.as_deref() == Some(version.as_str()),
                tag: r.tag_name,
                published_at: r.published_at,
                asset_name: asset.as_ref().map(|a| a.name.clone()),
                download_url: asset
                    .as_ref()
                    .map(|a| apply_mirror(&mirror, &a.browser_download_url)),
                size: asset.as_ref().map(|a| a.size),
                version,
            });
        }
    }

    // 本地已装但不在远端首页列表里的（更老版本）。
    for v in installed {
        if seen.insert(v.clone()) {
            out.push(RemoteVersion {
                active: active.as_deref() == Some(v.as_str()),
                tag: format!("v{v}"),
                version: v,
                published_at: String::new(),
                asset_name: None,
                download_url: None,
                size: None,
                installed: true,
            });
        }
    }

    Ok(VersionList {
        versions: out,
        remote_error,
    })
}

pub async fn check_update(
    app: AppHandle,
    state: AppState,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    let versions = list(app, state.clone()).await?;
    let current_version = active_version(&state);
    let latest_version = versions
        .versions
        .iter()
        .find(|version| version.download_url.is_some() || version.installed)
        .map(|version| version.version.clone());
    let update_available = current_version.is_some()
        && latest_version.is_some()
        && current_version.as_deref() != latest_version.as_deref();
    Ok(ProviderRuntimeUpdateStatus {
        provider_id: FRP_PROVIDER_ID.into(),
        runtime: "frpc".into(),
        current_version,
        latest_version,
        update_available,
        message: versions.remote_error.unwrap_or_default(),
    })
}

/// 下载并安装指定版本（首个安装的版本会自动设为激活）。
pub async fn install(app: AppHandle, state: AppState, version: String) -> AppResult<()> {
    // 新建版本目录必须严格校验；目录扫描的宽松规则只用于读取已有安装项。
    paths::validate_version(&version)?;
    let (os, arch) = platform();

    let release = fetch_remote()
        .await?
        .into_iter()
        .find(|r| r.tag_name.trim_start_matches('v') == version)
        .ok_or_else(|| AppError::Msg(format!("Version {version} was not found")))?;

    let asset = release
        .assets
        .iter()
        .find(|a| asset_matches(&a.name, os, arch))
        .cloned()
        .ok_or_else(|| {
            AppError::Msg(format!(
                "Version {version} does not include an asset for the current platform ({os}_{arch})"
            ))
        })?;

    // 校验和文件（frp_*_sha256_checksums.txt）。务必走 GitHub 直连下载，不套镜像：
    // 校验和是信任根，若也经第三方镜像下载，攻击者可同时替换二进制与校验和，校验形同虚设。
    let checksum_asset = release
        .assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().contains("checksums"))
        .cloned();

    let root = paths::frpc_root(&app)?;
    tokio::fs::create_dir_all(&root).await?;
    // 清扫临时下载文件：进程被杀/崩溃可能在 root 下留下 .download-*，这里先扫掉，
    // 避免无限累积（installed_versions 只看目录、跳过 dotfile，不会回收它们）。
    sweep_stale_downloads(&root).await;
    let is_zip = asset.name.ends_with(".zip");
    // 临时文件名带进程内单调递增计数器，保证并发安装（含同一版本）各自独立，
    // 不依赖系统时钟（时钟回退/低分辨率都不会导致碰撞）。
    let unique = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = root.join(format!(
        ".download-{version}-{}-{unique}.{}",
        std::process::id(),
        if is_zip { "zip" } else { "tar.gz" }
    ));

    let mirror = state.config.frp_settings().frpc_mirror.unwrap_or_default();
    let url = apply_mirror(&mirror, &asset.browser_download_url);
    // 下载失败/校验失败都要清掉临时文件，避免残留半截包。
    if let Err(e) = download_stream(&app, &url, &tmp, &version, asset.size).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }

    // 校验 + 解压：任一步出错都要删掉临时文件，避免残留未校验/半截的下载包。
    // 包进内部 async 块，出口统一清理一次，杜绝某条错误分支漏删（含 spawn_blocking 的 JoinError）。
    let version_dir = paths::version_dir(&app, &version)?;
    let verify_and_unpack = async {
        // 完整性校验：从校验和文件取出本资源的期望 SHA256，与下载内容比对。
        // 找不到校验和文件时拒绝安装（frp 官方 release 一定带），避免静默降级为“无校验”。
        let c = checksum_asset.ok_or_else(|| {
            AppError::Msg(format!(
                "Release {version} does not provide a SHA256 checksum file; refusing to install for safety"
            ))
        })?;
        let expected = fetch_expected_sha256(&c.browser_download_url, &asset.name).await?;
        let tmp_v = tmp.clone();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&tmp_v))
            .await
            .map_err(|e| AppError::Msg(e.to_string()))??;
        if !actual.eq_ignore_ascii_case(&expected) {
            return Err(AppError::Msg(format!(
                "Downloaded content failed verification (SHA256 mismatch) and was discarded. Expected {expected}, got {actual}"
            )));
        }
        let tmp_clone = tmp.clone();
        tokio::task::spawn_blocking(move || unpack(&tmp_clone, &version_dir, is_zip))
            .await
            .map_err(|e| AppError::Msg(e.to_string()))??;
        Ok(())
    };
    let result: AppResult<()> = verify_and_unpack.await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result?;

    // 第一个装好的版本顺便激活。
    let mut activated = false;
    if state.config.frp_settings().active_frpc_version.is_none() {
        save_active_frpc_version(&app, &state, Some(version.clone()))?;
        activated = true;
    }

    let _ = app.emit("versions-updated", &version);
    if activated {
        let _ = app.emit("frpc-version-activated", &version);
    }
    Ok(())
}

async fn download_stream(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    version: &str,
    expected_size: u64,
) -> AppResult<()> {
    // 下载是流式大响应：只设连接/读超时（卡死检测），不能设总超时——慢链路上下载
    // 一个 10MB+ 的包超过总超时会被掐断。
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    // 允许少量响应头误差，但绝不接受明显超过 GitHub asset 大小的响应，避免镜像异常
    // 或恶意响应导致持续写盘。最终仍以 SHA256 校验作为内容信任根。
    let max_size = expected_size.saturating_add(1024 * 1024);
    if expected_size > 0 && total > max_size {
        return Err(AppError::Msg(format!(
            "Download response size is unexpected: expected about {expected_size} bytes, got {total} bytes"
        )));
    }

    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        if expected_size > 0 && received > max_size {
            return Err(AppError::Msg(format!(
                "Downloaded content is too large: expected about {expected_size} bytes, received {received} bytes"
            )));
        }
        // 限频：每 256KB 或下载完成时推一次进度。
        if received - last_emit >= 256 * 1024 || (total > 0 && received >= total) {
            last_emit = received;
            let _ = app.emit(
                "frpc-download-progress",
                DownloadProgress {
                    version: version.to_string(),
                    received,
                    total,
                },
            );
        }
    }
    file.flush().await?;
    Ok(())
}

/// 下载并解析 frp 的 `*_sha256_checksums.txt`，取出指定资源名对应的期望 SHA256。
/// 校验和文件务必直连 GitHub（调用方传入 browser_download_url，不套镜像）。
/// 文件每行格式：`<sha256>  <filename>`（GNU coreutils sha256sum 风格）。
async fn fetch_expected_sha256(checksum_url: &str, asset_name: &str) -> AppResult<String> {
    // 校验和现在是强制步骤：必须带超时，否则 GitHub 卡住会让整个安装无限挂起。
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(30))
        .build()?;
    let text = client
        .get(checksum_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        // 文件名可能带 `*` 前缀（二进制模式标记）或路径前缀，取末段比对。
        let name = name.trim_start_matches('*');
        let base = name.rsplit('/').next().unwrap_or(name);
        if base == asset_name {
            return Ok(hash.to_string());
        }
    }
    Err(AppError::Msg(format!(
        "SHA256 for {asset_name} was not found in the checksum file"
    )))
}

/// 计算文件 SHA256，返回小写十六进制字符串。分块读取，避免把大文件整块载入内存。
fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// 从压缩包中只取出 frpc 可执行文件，放到 `version_dir/frpc[.exe]`。
fn unpack(archive: &Path, version_dir: &Path, is_zip: bool) -> AppResult<()> {
    fs::create_dir_all(version_dir)?;
    let target = version_dir.join(paths::exe_name());
    let temp = temp_exe_path(version_dir);
    let result = (|| -> AppResult<()> {
        if is_zip {
            unpack_zip(archive, &temp)?;
        } else {
            unpack_targz(archive, &temp)?;
        }
        #[cfg(unix)]
        set_executable(&temp)?;
        replace_file(&temp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn unpack_targz(archive: &Path, target: &Path) -> AppResult<()> {
    let f = fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let is_frpc = entry
            .path()?
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n == "frpc")
            .unwrap_or(false);
        if is_frpc {
            let mut out = fs::File::create(target)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Msg(
        "frpc executable was not found in the downloaded archive".into(),
    ))
}

fn unpack_zip(archive: &Path, target: &Path) -> AppResult<()> {
    let f = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| AppError::Msg(e.to_string()))?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| AppError::Msg(e.to_string()))?;
        let name = file.name().replace('\\', "/");
        if name.ends_with("/frpc.exe") || name == "frpc.exe" {
            let mut out = fs::File::create(target)?;
            std::io::copy(&mut file, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Msg(
        "frpc.exe was not found in the downloaded archive".into(),
    ))
}

fn temp_exe_path(version_dir: &Path) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    version_dir.join(format!(
        ".{}-{}-{seq}.tmp",
        paths::exe_name(),
        std::process::id()
    ))
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    fs::rename(temp, target)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    let backup = target.with_file_name(format!(
        ".{}-{}-{}.backup",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("frpc"),
        std::process::id(),
        TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
fn set_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm)?;
    Ok(())
}

pub fn activate(app: &AppHandle, state: &AppState, version: &str) -> AppResult<()> {
    if !paths::frpc_exe(app, version)?.exists() {
        return Err(AppError::Msg(format!("Version {version} is not installed")));
    }
    save_active_frpc_version(app, state, Some(version.to_string()))?;
    let _ = app.emit("frpc-version-activated", version);
    let _ = app.emit("versions-updated", version);
    Ok(())
}

pub fn remove(app: &AppHandle, state: &AppState, version: &str) -> AppResult<()> {
    let dir = paths::version_dir(app, version)?;
    if state.config.frp_settings().active_frpc_version.as_deref() == Some(version) {
        return Err(AppError::ActiveVersionInUse);
    }
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    let _ = app.emit("versions-updated", version);
    Ok(())
}

pub fn uninstall_active(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let Some(version) = state.config.frp_settings().active_frpc_version else {
        return Ok(());
    };
    frpc_service::stop_all(app, state)?;
    let has_active_runtime = {
        let frp = frp_state(state);
        let rt = frp.runtime.lock();
        rt.instances
            .values()
            .any(|instance| instance.status.is_active())
    };
    if has_active_runtime {
        return Err(AppError::FrpcRuntimeInUse);
    }

    let dir = paths::version_dir(app, &version)?;
    let trash = dir.with_file_name(format!(
        ".{version}-delete-{}-{}",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let moved_dir = if dir.exists() {
        fs::rename(&dir, &trash)?;
        true
    } else {
        false
    };
    let before = state.config.snapshot()?;
    state.config.set_active_frpc_version(None)?;
    if let Err(error) = crate::store::save_app_data(app, state) {
        state.config.replace(before);
        if moved_dir {
            let _ = fs::rename(&trash, &dir);
        }
        return Err(error);
    }
    if moved_dir {
        let _ = fs::remove_dir_all(&trash);
    }
    let _ = app.emit("frpc-version-activated", Option::<String>::None);
    let _ = app.emit("versions-updated", version);
    Ok(())
}

pub fn active_version(state: &AppState) -> Option<String> {
    state.config.frp_settings().active_frpc_version
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(name: &str) -> PathBuf {
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir()
            .join("tunnelx-version-tests")
            .join(name)
            .join(suffix);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn apply_mirror_only_rewrites_https() {
        let url =
            "https://github.com/fatedier/frp/releases/download/v0.58.0/frp_0.58.0_linux_amd64.tar.gz";
        assert_eq!(apply_mirror("", url), url);
        assert_eq!(apply_mirror("   ", url), url);
        // 明文 http 镜像被拒（防中间人投毒），退回直连。
        assert_eq!(apply_mirror("http://insecure.example", url), url);
        assert_eq!(
            apply_mirror("https://mirror.example.com", url),
            format!("https://mirror.example.com/{url}")
        );
        // 末尾斜杠不应产生双斜杠。
        assert_eq!(
            apply_mirror("https://mirror.example.com/", url),
            format!("https://mirror.example.com/{url}")
        );
    }

    #[test]
    fn parse_ver_extracts_numeric_triple() {
        assert_eq!(parse_ver("0.52.0"), (0, 52, 0));
        assert_eq!(parse_ver("1.2.3"), (1, 2, 3));
        assert_eq!(parse_ver("0.58"), (0, 58, 0));
        assert_eq!(parse_ver("0.61.0-rc.1"), (0, 61, 0));
    }

    #[test]
    fn supports_json_gates_on_0_52() {
        assert!(supports_json("0.52.0"));
        assert!(supports_json("0.61.0"));
        assert!(supports_json("1.0.0"));
        assert!(!supports_json("0.51.0"));
        assert!(!supports_json("0.40.0"));
    }

    #[test]
    fn asset_matches_platform_archives_only() {
        assert!(asset_matches(
            "frp_0.58.0_linux_amd64.tar.gz",
            "linux",
            "amd64"
        ));
        assert!(asset_matches(
            "frp_0.58.0_windows_amd64.zip",
            "windows",
            "amd64"
        ));
        assert!(!asset_matches(
            "frp_0.58.0_linux_arm64.tar.gz",
            "linux",
            "amd64"
        ));
        // 校验和等非压缩包不能被当成安装资源。
        assert!(!asset_matches(
            "frp_0.58.0_sha256_checksums.txt",
            "linux",
            "amd64"
        ));
        assert!(!asset_matches(
            "frp_0.58.0_darwin_amd64.tar.gz",
            "linux",
            "amd64"
        ));
    }

    #[test]
    fn replace_file_swaps_completed_temp_into_place() {
        let dir = unique_test_dir("replace-success");
        let target = dir.join("frpc");
        let temp = dir.join(".frpc.tmp");
        fs::write(&target, b"old").unwrap();
        fs::write(&temp, b"new").unwrap();

        replace_file(&temp, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
    }

    #[test]
    fn replace_file_keeps_existing_target_when_temp_is_missing() {
        let dir = unique_test_dir("replace-fail");
        let target = dir.join("frpc");
        let temp = dir.join(".missing.tmp");
        fs::write(&target, b"old").unwrap();

        assert!(replace_file(&temp, &target).is_err());

        assert_eq!(fs::read(&target).unwrap(), b"old");
    }
}
