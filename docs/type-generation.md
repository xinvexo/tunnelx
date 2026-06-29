# Type Generation Evaluation

The Rust and TypeScript domain models currently serve different purposes:

- Rust `src-tauri/src/domain/*` types describe sparse persisted/wire data. Many fields
  are omitted with `skip_serializing_if` so provider-native runtime config can stay
  compact and let the underlying tool apply its own defaults.
- TypeScript `src/domain/*` types describe hydrated editor state. The UI fills omitted
  fields with stable defaults so forms, toggles, and nested editors can render without
  optional-field checks everywhere.

Because of that split, directly replacing the current frontend domain files with
`ts-rs` output would be risky. It would turn many editor fields optional or nullable,
and could regress the hydration logic that prevents false dirty states and missing
plugin inputs.

Recommended path:

1. Generate backend DTO bindings only, for command responses and request payloads.
   Keep generated files under `src/domain/generated/` and treat the existing editor
   models as adapters on top of those DTOs.
2. Start with stable DTOs such as provider descriptors, tunnel resources, runtime
   status, metrics, version info, config-check results, and update status. Avoid
   provider-native editor models until the sparse wire shape and hydrated editor shape
   are separated explicitly.
3. Add a CI check that fails when generated bindings are stale. A small `cargo test`
   export test or an `xtask` command is enough; do not depend on developers remembering
   to regenerate files manually.
4. Once DTO generation is stable, consider deriving TypeScript bindings for persisted
   provider data and adding explicit `hydrate*` adapter tests on the frontend side.

Decision for now: do not perform a broad `ts-rs` conversion in this change set. The
safe first milestone is generated command DTOs plus stale-binding checks.
