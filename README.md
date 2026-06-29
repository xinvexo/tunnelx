# TunnelX

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Release](https://img.shields.io/github/v/release/xinvexo/tunnelx)](https://github.com/xinvexo/tunnelx/releases)
[![Downloads](https://img.shields.io/github/downloads/xinvexo/tunnelx/total)](https://github.com/xinvexo/tunnelx/releases)
[![CI](https://github.com/xinvexo/tunnelx/actions/workflows/ci.yml/badge.svg)](https://github.com/xinvexo/tunnelx/actions/workflows/ci.yml)

English | [简体中文](README.zh-CN.md)

A desktop platform for managing intranet tunnel providers, built with Tauri + Vue + TypeScript.

TunnelX gives frp, Cloudflare Tunnel, ngrok, cpolar, and Pinggy one unified workspace: create connections, configure tunnels, endpoints, or ingress rules, start/stop with one click, stream logs, and watch runtime metrics without jumping between provider-specific tools.

## Provider-based tunnel management

TunnelX treats every tunnel implementation as a provider. The platform owns connection lifecycle, logs, metrics, and navigation; each provider only supplies its native settings and tunnel-editing surface.

- **Use multiple providers in one workspace** — frp, Cloudflare, ngrok, cpolar, and Pinggy connections live in the same All Connections view.
- **Run connections in parallel** — Start, stop, and monitor provider-managed tunnel processes independently. One connection going down doesn't affect the others.
- **A workspace per connection** — Switch between connections from the sidebar; each keeps its own Overview / Tunnels / Logs / Settings tabs and live status.
- **Provider-native controls where they matter** — frp exposes proxy configuration and version management; Cloudflare exposes account, credentials, named tunnel, DNS route, and ingress management; ngrok, cpolar, and Pinggy keep their token, region/server, and endpoint-type settings.

## Features

- **Multi-provider connection management** — Create, edit, delete, start, stop, and monitor connections from supported providers through the same lifecycle controls
- **Visual tunnel config** — Full support for TCP / UDP / HTTP / HTTPS / TCPMUX / STCP / SUDP / XTCP, covering TLS, encryption, compression, health checks, load balancing, OIDC auth, client plugins, and more frp features
- **Cloudflare Tunnel support** — Configure account/API token, run `cloudflared login`, manage generated credentials, sync named tunnels, edit ingress rules, and write DNS routes
- **Hosted tunnel provider support** — ngrok supports HTTP / TCP / TLS; cpolar supports HTTP / TCP; Pinggy supports HTTP / TCP / UDP / TLS / TLSTCP. Public endpoint allocation still depends on each provider's account plan and server-side capabilities
- **One-click start/stop** — Start/stop supported connections, stream logs live, with per-connection running status
- **Live traffic monitoring** — Real-time up/down speed and totals per tunnel, with a sparkline in the sidebar. Traffic for TCP/UDP/HTTP/HTTPS tunnels is measured by routing it through a loopback-only relay that runs **inside the watchdog sidecar** (transparent forwarding), so traffic-monitored tunnels keep forwarding even if the GUI process crashes
- **Graceful stop** — On exit/stop, disconnects via the frpc admin API so frps deregisters tunnels immediately, avoiding "proxy already exists" on reconnect
- **Version management** — Download, install, and switch frpc versions from GitHub online
- **System tray** — Minimize to tray on close, silent start, auto-connect, background residency
- **Lightweight mode** — Destroy the webview on close to free memory; rebuild on demand from the tray icon
- **Bilingual UI** — Built-in English / 简体中文, follows your system language by default
- **Auto-update** — Optional check for new versions on startup, one-click download and install

## Download

Grab the latest installer for your platform from the [Releases](https://github.com/xinvexo/tunnelx/releases) page.

## Installation

TunnelX is not yet code-signed or notarized, so your OS may warn on first launch:

- **macOS** — If you see *"TunnelX is damaged and can't be opened"*, clear the quarantine flag, then open normally:
  ```bash
  xattr -dr com.apple.quarantine /Applications/TunnelX.app
  ```
  (or right-click the app → **Open** → **Open**).
- **Windows** — On the SmartScreen prompt, click **More info** → **Run anyway**.
- **Linux** — Make the AppImage executable (`chmod +x`), or install the `.deb` / `.rpm` package.

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | Vue 3 + TypeScript + Vite |
| State | Pinia |
| Router | Vue Router |
| UI | UnoCSS + Reka UI + Iconify |
| Backend | Rust (Tauri v2, tokio, reqwest, serde) |
| Watchdog sidecar | Rust (`tunnelx-watchdog`) |

### How runtime cleanup stays safe

The app launches a **single** shared watchdog sidecar (`tunnelx-watchdog`) that supervises provider processes over a line-delimited JSON protocol. The watchdog handles spawning, log forwarding, exit reaping, the loopback traffic relay (per-tunnel byte counting), and crash-safe cleanup:

- **Windows** — All frpc processes go into one `KILL_ON_JOB_CLOSE` Job Object; when the main process handle closes, the OS terminates them all
- **Linux** — Each frpc sets `PR_SET_PDEATHSIG=SIGKILL`, so it dies the moment its parent does
- **All Unix** — Each frpc gets its own session/process group, terminated as a group on stop to avoid orphaned descendants

Whether the main process exits normally, crashes, or is force-killed, frp child processes are cleaned up. Because the traffic relay also lives in the sidecar rather than the GUI, tunnels with live traffic monitoring keep serving traffic if the GUI crashes — only the supervising sidecar's exit tears them down. Other providers report their runtime state through the same TunnelX lifecycle model while keeping provider-native process details inside their module.

## Local Verification

Use the normal build and test commands for local verification:

```bash
pnpm build
cargo test --workspace
```

## Data Storage

TunnelX stores platform data in a local SQLite database managed by the Tauri SQL plugin. Connections, provider settings, tunnels, ingress rules, endpoints, and ordering are stored in queryable relational tables. Provider-specific runtime files stay under provider-owned directories, such as generated frpc runtime config files and Cloudflare managed credentials/config files.

> **Note:** Sensitive fields such as auth tokens, tunnel secret keys, HTTP passwords, OIDC client secrets, API tokens, and credential references are stored locally and may be plaintext depending on the provider's native format. This data lives only on your machine and is never uploaded to any TunnelX server.

## Building from Source

<details>
<summary>Prerequisites and build commands</summary>

Requires [Node.js](https://nodejs.org/) ≥ 18, [pnpm](https://pnpm.io/), [Rust](https://www.rust-lang.org/), and the [Tauri CLI](https://v2.tauri.app/). The watchdog sidecar is a Rust crate compiled via Cargo by `pnpm build:watchdog` — no extra toolchain required.

```bash
pnpm install          # install frontend dependencies
pnpm build:watchdog   # build the watchdog sidecar (auto-invoked before dev/build)
pnpm tauri:dev        # start the dev environment
pnpm tauri:build      # production build
```

Recommended IDE: [VS Code](https://code.visualstudio.com/) + [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

</details>

## Acknowledgements

- [frp](https://github.com/fatedier/frp) — one of the tunnel providers managed by TunnelX
- [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) — named tunnel and ingress provider support
- [ngrok](https://ngrok.com/) — hosted HTTP/TCP/TLS tunnel support
- [cpolar](https://www.cpolar.com/) — hosted HTTP/TCP tunnel support
- [Pinggy](https://pinggy.io/) — hosted HTTP/TCP/UDP/TLS/TLSTCP tunnel support
- [Tauri](https://tauri.app/) — the cross-platform desktop app framework

## Contributing

Contributions are welcome! Please read the [Contributing Guide](.github/CONTRIBUTING.md) first.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## License

[MIT](LICENSE)
