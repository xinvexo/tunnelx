# TunnelX Provider Architecture

TunnelX is an intranet tunnel aggregation platform. frp, Cloudflare Tunnel, ngrok, cpolar, and Pinggy are providers: they implement the tunnel capability, but they are not the product boundary.

## Core Rules

- TunnelX owns the common concepts: provider, tunnel resource, ingress rule, route result, provider status, provider capability, lifecycle state, logs, and metrics.
- Provider-specific code lives under `providers/<provider-id>` on both backend and frontend.
- Provider-specific entities, services, runtime state, command entrypoints, native helpers, and native lifecycle code must stay inside that provider module.
- Shared UI and commands should call the provider contract first. Provider-specific APIs are acceptable for native capabilities that do not belong in the common contract yet.
- Persisted provider-specific data lives in provider-owned SQLite tables. The platform does not keep provider fields such as frp profiles, Cloudflare account data, or hosted-provider endpoint data at the root.
- Runtime provider state is registered through the platform `ProviderStateStore`; `AppState` must not contain provider-specific fields.
- TunnelX stores provider data in a local SQLite database with a standard connection order plus provider-owned tables. Provider-specific fields belong in each provider module; provider modules should generate and normalize their own runtime files without adding user-facing import/export paths to the common flow.
- Provider data can keep its native shape inside the provider module, but it must be convertible to `TunnelResource`.
- New providers must declare capabilities. Common UI should show shared actions based on capabilities; provider-native setup and management can stay inside that provider module.
- Runtime observations such as traffic, upload/download speed, and provider process memory must flow through `ProviderMetrics` / `RuntimeMetrics`.
- Provider management belongs in the unified "All Connections" workspace. Do not add a top-level page or sidebar item for every provider.
- Platform process supervision belongs to the generic `process_watchdog` service and `tunnelx-watchdog` sidecar. Providers may generate provider-specific stop or cleanup intent, but it must be expressed as generic protocol actions such as HTTP requests, JSON-list delete matches, and local file removal. The sidecar must not contain provider-specific API clients, entity types, or cleanup branches.
- `process_watchdog` and `tunnelx-watchdog` are split by responsibility. The app service owns sidecar discovery, client commands, event forwarding, and handle state in separate modules; the sidecar owns argument parsing, event emission, supervisor loop, managed process lifecycle, platform-specific process control, and cleanup execution in separate modules.

## Backend Layout

```text
src-tauri/src/providers/
  contract.rs              # common trait and DTOs
  registry.rs              # provider lookup and dispatch
  frp/
    commands/              # frp-specific Tauri command entrypoints
    data.rs                # frp SQLite persistence and provider data normalization
    domain/                # frp entities and frpc config conversion
    services/              # frpc lifecycle, traffic, version
    runtime_state.rs       # frp runtime state
    state.rs               # in-memory frp provider state
    provider.rs            # TunnelProvider implementation
  cloudflare/
    account/               # authorization window, cert parsing, credentials management
    commands.rs            # Cloudflare-specific Tauri command entrypoints
    data/                  # persisted Cloudflare provider data and save/emit helpers
    domain/                # Cloudflare entities
    environment/           # cloudflared discovery, install, uninstall, CLI helpers
    services/              # Cloudflare provider services facade
    tunnel/                # API, config generation, ingress validation, runtime, remote cleanup
    paths.rs               # Cloudflare provider filesystem layout
    provider.rs            # TunnelProvider implementation
  ngrok/
    commands.rs            # ngrok-specific environment and credential entrypoints
    data.rs                # ngrok tunnel/endpoint persistence mapping
    domain.rs              # ngrok entities
    environment.rs         # ngrok runtime discovery, install, uninstall
    runtime.rs             # ngrok config generation and process lifecycle
    provider.rs            # TunnelProvider implementation
  cpolar/
    commands.rs            # cpolar-specific environment and credential entrypoints
    data.rs                # cpolar tunnel/endpoint persistence mapping
    domain.rs              # cpolar entities
    environment.rs         # cpolar runtime discovery, install, uninstall
    runtime.rs             # cpolar config generation and process lifecycle
    provider.rs            # TunnelProvider implementation
  pinggy/
    commands.rs            # Pinggy-specific environment and credential entrypoints
    credentials.rs         # Pinggy token verification
    data.rs                # Pinggy tunnel/endpoint persistence mapping
    domain.rs              # Pinggy entities
    environment.rs         # Pinggy runtime discovery, install, uninstall
    runtime.rs             # Pinggy CLI lifecycle and public URL parsing
    provider.rs            # TunnelProvider implementation
```

The core interface is `TunnelProvider`. Commands should call `providers::registry`, which dispatches by `provider_id`.

Common metric commands:

- `provider_metrics(provider_id)` returns one provider's memory and traffic snapshot.
- `runtime_metrics()` returns the global TunnelX metrics aggregate across providers.

Common lifecycle commands:

- `provider_status(provider_id)` returns provider availability and native detail metadata.
- `provider_login(provider_id)` runs a provider login/auth flow when the provider supports it.
- `provider_create_tunnel(provider_id, input)` creates a platform tunnel resource.
- `provider_update_tunnel(provider_id, tunnel)` updates a platform tunnel resource.
- `provider_delete_tunnel(provider_id, id, remote)` deletes a platform tunnel resource.
- `provider_route_dns(provider_id, id, hostname)` writes a provider-owned DNS route when supported.
- `provider_start_tunnel(provider_id, id)` starts a platform tunnel resource.
- `provider_stop_tunnel(provider_id, id)` stops a platform tunnel resource.
- `provider_tunnel_status(provider_id, id)` returns `TunnelRuntimeInfo`.
- `provider_tunnel_logs(provider_id, id)` returns bounded runtime logs.

Provider implementations own their native process details, but they must report state through the platform contract. The UI should not call provider-specific lifecycle commands when a `provider_*` command exists.
`src-tauri/src/services/process_watchdog/` owns the single platform sidecar process and forwards sidecar events through the provider registry. Provider modules own the interpretation of those events for their own runtime state.

## Frontend Layout

```text
src/providers/
  contract.ts              # common provider DTOs
  api.ts                   # common provider invoke API
  connections.ts           # unified provider tunnel resources and lifecycle state
  routes.ts                # provider-aware connection route helpers
  module.ts                # frontend provider module contract
  registry.ts              # registered frontend provider modules
  frp/
    api/                   # frp-specific invoke wrappers
    domain/                # frp frontend entities
    stores/                # frp provider stores
    components/            # frp-only editors
    views/                 # frp settings/tunnels/environment panels
    module.ts              # frp frontend provider module
  cloudflare/              # Cloudflare-specific API/domain/stores/components/views/module
  ngrok/                   # ngrok-specific API/domain/components/views/module
  cpolar/                  # cpolar-specific API/domain/components/views/module
  pinggy/                  # Pinggy-specific API/domain/components/views/module
src/layouts/
  ProviderWorkspaceLayout.vue # shared connection topbar, lifecycle controls, tabs
src/views/
  ConnectionsView.vue      # all-connections list only
  ConnectionOverview.vue   # shared provider resource overview
  ConnectionTunnels.vue    # shared tab wrapper; delegates provider-specific tunnel editors
  ConnectionLogs.vue       # shared provider log viewer
  ConnectionSettings.vue   # shared tab wrapper; delegates provider-specific settings
src/components/
  ConnectionCreateDialog.vue # creates a connection by name + provider type
```

`/connections` is the platform workspace. Provider-specific panels can be embedded there, but they should not become standalone primary navigation items.
The default all-connections view should read from `useConnectionStore` and operate through `provider_*` commands. Creating a connection is a dialog that asks only for the connection name and provider type, then dispatches to `provider_create_tunnel`.
Connection details use provider-aware routes: `/connections/:providerId/:id/overview`, `/tunnels`, `/logs`, and `/settings`. The shell, tabs, lifecycle controls, and logs are platform-owned; provider-native components only fill provider-specific settings or tunnel-editing surfaces.
Provider-specific frontend stores and panels are exposed through `ProviderFrontendModule`. Public components should ask the registry for icons, endpoint text, tunnel preview rows, settings panels, tunnel panels, version panels, init, connection hydration, and connection cleanup instead of importing provider files directly.
Provider resource changes should emit `provider-tunnels-updated`. Runtime status and log streams should emit `provider-tunnel-status-changed` and `provider-tunnel-log`; provider-native events may exist inside the provider module, but platform UI should not depend on them.

## Adding A Provider

1. Add backend implementation under `src-tauri/src/providers/<id>`.
2. Implement `TunnelProvider`.
3. Register it in `src-tauri/src/providers/registry.rs`.
4. Keep native entities/services/state under that provider module.
5. Add frontend provider-specific files under `src/providers/<id>` only when common UI is insufficient.
6. Prefer extending `providers/contract.*` over creating unrelated one-off commands.

## Current Architecture State

- Cloudflare has been moved under the provider structure and exposed through the common provider registry.
- ngrok, cpolar, and Pinggy have been added to the same provider registry and common connection workspace.
- Cloudflare UI is embedded in the unified `/connections` workspace instead of being a separate sidebar page.
- Connection creation is standardized through `ConnectionCreateDialog`; frp, Cloudflare, ngrok, cpolar, and Pinggy create through `provider_create_tunnel`.
- All provider connection details share the same provider-aware topbar and `overview / tunnels / logs / settings` tabs.
- Cloudflare backend services are split by responsibility: account, data persistence, API, CLI, config, credentials, files, hostname validation, remote cleanup, runtime, and tunnel operations.
- Cloudflare implements the standard lifecycle through `provider_start_tunnel` / `provider_stop_tunnel`, backed by a platform-level provider runtime state.
- Cloudflare status, login, create, update, delete, DNS route, runtime status, and logs now flow through the common provider contract. Cloudflare-specific commands are reserved for account, credentials, and remote account/tunnel discovery features.
- ngrok supports HTTP/TCP/TLS endpoints through its provider module. Account-plan restrictions are treated as provider/runtime outcomes rather than platform lifecycle failures.
- cpolar supports HTTP/TCP endpoints through its provider module.
- Pinggy supports HTTP/TCP/UDP/TLS/TLSTCP endpoints through its provider module. Public URL parsing accepts `https://`, `http://`, `tcp://`, `udp://`, and `tls://` forms.
- frp exposes profiles as platform tunnel resources; proxies are profile internals and metric dimensions, not the lifecycle resource. frp also implements `provider_create_tunnel` so the UI does not call frp profile creation directly.
- frp start, stop, status, logs, create, and delete now flow through the common provider contract. frp-specific commands are reserved for native profile editing, proxy editing, config checks, traffic details, and frpc version management.
- frp entities, config conversion, runtime state, frpc lifecycle, traffic relay orchestration (the relay's loopback socket forwarding itself runs in the watchdog sidecar — see below; the frp module owns relay-port assignment, proxy-target rewriting, and stats collection), profile service, and version service now live under `src-tauri/src/providers/frp`.
- frp frontend API/domain/stores/components/views now live under `src/providers/frp`; the public frontend uses `src/providers/registry.ts` to reach provider panels and lifecycle hydration.
- Frontend provider modules can implement `cleanupConnection` so the platform lifecycle can clear provider-owned UI/runtime caches after a connection is deleted.
- Persisted platform and provider data now live in queryable SQLite tables (`frp_*`, `cloudflare_*`, `ngrok_*`, `cpolar_*`, `pinggy_*`, plus `connection_order` and platform settings) instead of a root provider JSON blob.
- Provider runtime state now lives in the shared provider runtime state store; frp keeps provider-owned traffic/runtime helpers where needed.
- Provider command entrypoints now live under their provider modules; the top-level backend `commands` module is reserved for platform commands.
- The top-level backend `services` module is reserved for platform services such as process metrics and app updates.
- The platform watchdog service is modularized under `src-tauri/src/services/process_watchdog`: `binary` owns sidecar discovery, `client` owns command transport and shutdown, `events` owns event forwarding, and `state` owns the sidecar handle.
- The platform watchdog sidecar is `tunnelx-watchdog` and uses provider-neutral protocol types. frp expresses admin shutdown as a generic HTTP stop request; Cloudflare expresses abnormal-exit DNS / named tunnel cleanup as generic HTTP and JSON-list delete actions plus local file removal.
- The sidecar code is modularized under `tunnelx-watchdog/src`: `supervisor` owns the command loop, `process` owns child process lifecycle, `cleanup` owns generic cleanup actions, `platform` owns Unix/Windows process differences, `emitter` owns line-delimited event output, and `relay` owns the loopback per-tunnel traffic relay (transparent TCP/UDP forwarding with byte counting), so traffic monitoring survives a GUI crash.
