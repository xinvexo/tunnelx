# TunnelX

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Release](https://img.shields.io/github/v/release/xinvexo/tunnelx)](https://github.com/xinvexo/tunnelx/releases)
[![Downloads](https://img.shields.io/github/downloads/xinvexo/tunnelx/total)](https://github.com/xinvexo/tunnelx/releases)
[![CI](https://github.com/xinvexo/tunnelx/actions/workflows/ci.yml/badge.svg)](https://github.com/xinvexo/tunnelx/actions/workflows/ci.yml)

[English](README.md) | 简体中文

一个用于聚合管理多种内网穿透提供方的桌面平台，基于 Tauri + Vue + TypeScript 构建。

TunnelX 将 frp、Cloudflare Tunnel、ngrok、cpolar 和 Pinggy 放进统一工作区：创建连接、配置隧道/endpoint/ingress、一键启动/停止、查看实时日志和运行指标，不需要在不同工具之间来回切换。

## 基于提供方的隧道管理

TunnelX 把每种内网穿透实现都当作 provider。平台负责连接生命周期、日志、指标和导航；各 provider 只提供自己的原生设置和隧道编辑能力。

- **统一管理多个 provider**：frp、Cloudflare、ngrok、cpolar 和 Pinggy 连接都放在同一个“全部连接”视图里。
- **并行运行多个连接**：每个 provider 管理的隧道进程都能独立启动、停止和监控，一个连接异常不会影响其它连接。
- **每个连接都有独立工作区**：从侧边栏切换连接后，可以查看各自的概览、隧道、日志和设置。
- **必要时保留原生能力**：frp 提供代理配置和版本管理；Cloudflare 提供账号、credentials、named tunnel、DNS route 和 ingress 管理；ngrok、cpolar 和 Pinggy 保留各自的 token、region/server 与 endpoint 类型设置。

## 功能特性

- **多 provider 连接管理**：通过同一套生命周期控件创建、编辑、删除、启动、停止和监控受支持 provider 的连接。
- **可视化隧道配置**：支持 TCP / UDP / HTTP / HTTPS / TCPMUX / STCP / SUDP / XTCP，覆盖 TLS、加密、压缩、健康检查、负载均衡、OIDC 认证、客户端插件等 frp 常用能力。
- **Cloudflare Tunnel 支持**：配置账号/API Token、执行 `cloudflared login`、管理生成的 credentials、同步 named tunnel、编辑 ingress，并写入 DNS route。
- **托管隧道服务支持**：ngrok 支持 HTTP / TCP / TLS；cpolar 支持 HTTP / TCP；Pinggy 支持 HTTP / TCP / UDP / TLS / TLSTCP。具体能否分配公网入口仍取决于各 provider 的账号套餐和服务端能力。
- **一键启动/停止**：按连接启动或停止受支持的 provider，实时流式日志，并展示连接运行状态。
- **实时流量监控**：按隧道展示实时上下行速度和累计流量，侧边栏带迷你曲线图。TCP/UDP/HTTP/HTTPS 隧道通过仅监听 loopback 的透明中继统计流量，该中继**运行在 watchdog sidecar 进程内**，因此即使 GUI 进程崩溃，开启流量监控的隧道仍能继续转发。
- **优雅停止**：停止时优先通过 frpc admin API 断开连接，让 frps 立即注销隧道，减少重连时的 “proxy already exists”。
- **版本管理**：可在线从 GitHub 下载、安装、切换 frpc 版本。
- **系统托盘**：关闭窗口时最小化到托盘，支持静默启动、自动连接和后台驻留。
- **轻量模式**：关闭窗口后销毁 webview 释放内存，再次点击托盘时按需重建。
- **双语界面**：内置 English / 简体中文，默认跟随系统语言。
- **自动更新**：可选启动时检查新版本，并一键下载安装。

## 下载

请从 [Releases](https://github.com/xinvexo/tunnelx/releases) 页面下载适合你平台的最新安装包。

## 安装

TunnelX 目前尚未进行代码签名或 notarize，首次启动时操作系统可能会提示风险：

- **macOS**：如果看到 “TunnelX is damaged and can't be opened”，可以清除隔离标记后再打开：
  ```bash
  xattr -dr com.apple.quarantine /Applications/TunnelX.app
  ```
  也可以右键应用，选择 **打开**，再确认打开。
- **Windows**：遇到 SmartScreen 提示时，点击 **更多信息**，再点击 **仍要运行**。
- **Linux**：AppImage 需要先添加执行权限（`chmod +x`），也可以安装 `.deb` / `.rpm` 包。

## 技术栈

| 层级 | 技术 |
|---|---|
| 前端 | Vue 3 + TypeScript + Vite |
| 状态管理 | Pinia |
| 路由 | Vue Router |
| UI | UnoCSS + Reka UI + Iconify |
| 后端 | Rust（Tauri v2、tokio、reqwest、serde） |
| Watchdog sidecar | Rust（`tunnelx-watchdog`） |

### 运行时清理如何保证安全

应用会启动一个共享的 watchdog sidecar（`tunnelx-watchdog`），通过按行分隔的 JSON 协议托管各 provider 的运行进程。watchdog 负责启动进程、转发日志、回收退出状态、loopback 流量中继（按隧道计字节）和崩溃清理：

- **Windows**：所有 frpc 进程都加入同一个带 `KILL_ON_JOB_CLOSE` 的 Job Object，主进程句柄关闭后系统会终止它们。
- **Linux**：每个 frpc 设置 `PR_SET_PDEATHSIG=SIGKILL`，父进程消失时立即退出。
- **所有 Unix 系统**：每个 frpc 都拥有独立 session/process group，停止时按进程组终止，避免留下子进程。

无论主进程正常退出、崩溃还是被强制结束，frp 子进程都会被清理。由于流量中继也跑在 sidecar 而非 GUI 里，GUI 崩溃时开启流量监控的隧道仍能继续服务，只有托管它们的 sidecar 退出时才会被拆除。其它 provider 通过同一套 TunnelX 生命周期模型上报运行态，同时把原生进程细节收敛在自己的模块内。

## 本地验证

本地验证使用常规构建和测试命令：

```bash
pnpm build
cargo test --workspace
```

## 数据存储

连接、provider 数据、隧道和设置会保存在系统应用数据目录下的本地 SQLite 数据库中。provider 专属运行文件会放在各自的目录下，例如 frpc 运行时配置文件，以及 Cloudflare 托管的 credentials/config 文件。

> **注意：** auth token、隧道 secret key、HTTP 密码、OIDC client secret、API Token、credential 引用等敏感信息会保存在本地，具体是否明文取决于 provider 的原生格式。这些数据只保存在你的机器上，不会上传到任何 TunnelX 服务器。

## 从源码构建

<details>
<summary>前置要求和构建命令</summary>

需要 [Node.js](https://nodejs.org/) ≥ 18、[pnpm](https://pnpm.io/)、[Rust](https://www.rust-lang.org/) 和 [Tauri CLI](https://v2.tauri.app/)。watchdog sidecar 是一个 Rust crate，会由 `pnpm build:watchdog` 通过 Cargo 编译，不需要额外工具链。

```bash
pnpm install          # 安装前端依赖
pnpm build:watchdog   # 构建 watchdog sidecar（dev/build 前会自动调用）
pnpm tauri:dev        # 启动开发环境
pnpm tauri:build      # 生产构建
```

推荐 IDE：[VS Code](https://code.visualstudio.com/) + [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)。

</details>

## 致谢

- [frp](https://github.com/fatedier/frp)：TunnelX 管理的隧道 provider 之一。
- [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)：提供 named tunnel 和 ingress 能力。
- [ngrok](https://ngrok.com/)：提供托管 HTTP/TCP/TLS 隧道能力。
- [cpolar](https://www.cpolar.com/)：提供托管 HTTP/TCP 隧道能力。
- [Pinggy](https://pinggy.io/)：提供托管 HTTP/TCP/UDP/TLS/TLSTCP 隧道能力。
- [Tauri](https://tauri.app/)：跨平台桌面应用框架。

## 贡献

欢迎贡献代码和反馈问题。开始前请阅读 [贡献指南](.github/CONTRIBUTING.md)。

## 变更记录

版本历史见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

[MIT](LICENSE)
