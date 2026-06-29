# Changelog

All notable changes to TunnelX are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses semantic version tags when publishing releases.

## [Unreleased]

### Added

- Added a provider-based TunnelX platform model so frp, Cloudflare Tunnel, ngrok, cpolar, and Pinggy can share lifecycle, logs, metrics, and connection navigation.
- Added Cloudflare Tunnel account, credentials, named tunnel, DNS route, ingress, and local runtime management.
- Added ngrok HTTP/TCP/TLS connection management.
- Added cpolar HTTP/TCP connection management.
- Added Pinggy HTTP/TCP/UDP/TLS/TLSTCP connection management.
- Added shared provider metrics aggregation for app memory, provider memory, upload/download totals, and per-tunnel samples.
- Added smoke scripts for local frp all-type verification and real-provider connectivity checks.

### Changed

- Moved frp backend/frontend code under provider-owned modules instead of treating frp as the app boundary.
- Routed provider create/delete/start/stop/status/log operations through the common provider contract.
- Removed the old frp config import/export flow; frp share links remain provider-specific.
- Renamed project metadata and documentation around TunnelX as a multi-provider intranet tunnel platform.

### Fixed

- Fixed stale provider runtime state after deleting a connection.
- Fixed CI compatibility for non-macOS builds and strict clippy warnings.
- Fixed share decoding so static-file, Unix socket, and TLS certificate path fields are surfaced before importing a shared frp tunnel.

## [0.1.0]

### Added

- Initial TunnelX desktop application.
- Multi-connection frpc profile management.
- Visual tunnel configuration for TCP, UDP, HTTP, HTTPS, TCPMUX, STCP, SUDP, and XTCP.
- Live logs, per-tunnel status, and local traffic monitoring.
- Shared watchdog sidecar for crash-safe frpc process supervision and the in-sidecar loopback traffic relay.
- frpc version download, import, activation, and removal.
- System tray, auto-connect, silent start, lightweight mode, and bilingual UI.
- Tauri updater integration and multi-platform release workflow.
