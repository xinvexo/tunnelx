# Contributing to TunnelX

Thanks for taking the time to improve TunnelX. This project is a Tauri desktop app with a Vue frontend, a Rust backend, and a Rust watchdog sidecar.

## Development Setup

Prerequisites:

- Node.js 18 or newer
- pnpm
- Rust stable
- Platform dependencies required by Tauri v2

Install dependencies and start the app:

```bash
pnpm install
pnpm build:watchdog
pnpm tauri:dev
```

Useful checks:

```bash
pnpm build
pnpm check:scripts
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`pnpm tauri:build` runs the full production build. Release signing requires the Tauri updater signing key, so local unsigned builds may stop at updater artifact signing unless the signing environment variables are configured.

## Project Layout

- `src/` contains the Vue app, stores, routes, domain models, and UI components.
- `src-tauri/` contains the Tauri Rust backend, commands, services, persisted domain types, and bundle configuration.
- `tunnelx-watchdog/` contains the watchdog sidecar that supervises provider processes.
- `tunnelx-watchdog-protocol/` contains the shared line-delimited JSON protocol types.
- `scripts/` contains local build helpers.

## Coding Guidelines

- Keep frontend domain hydration explicit. Backend Rust types are sparse wire/persistence data, while TypeScript editor models are hydrated form state.
- Keep Tauri commands thin. Put behavior in `src-tauri/src/services/*` and domain conversion in `src-tauri/src/domain/*`.
- Avoid storing or logging sensitive values such as auth tokens, tunnel secret keys, HTTP passwords, OIDC secrets, and updater signing keys.
- Keep frpc process lifecycle changes conservative. The watchdog is responsible for process ownership, log forwarding, and cleanup.
- Keep Rust unit tests next to the source they exercise in the same file under `#[cfg(test)] mod tests`. Use `tests/` only for integration tests that intentionally exercise public APIs from a separate crate.
- Run formatting and checks before opening a pull request.

## Pull Requests

Before submitting a PR:

1. Describe the user-visible change and the reason for it.
2. Mention any platform-specific behavior you touched.
3. Include screenshots for UI changes when possible.
4. List the checks you ran.

Small focused PRs are easier to review than broad refactors mixed with behavior changes.

## Reporting Issues

When reporting a bug, include:

- TunnelX version and operating system
- frpc version
- Steps to reproduce
- Expected and actual behavior
- Relevant logs with secrets removed

For configuration problems, please remove or mask tokens, passwords, domains you do not want public, and private addresses before posting.
