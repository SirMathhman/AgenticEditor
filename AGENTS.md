# AgenticEditor

Tauri v2 + Solid.js + TypeScript desktop app: a file browser/editor with an
OpenRouter-backed chat panel. The Rust backend is the only component allowed to
touch the OS, filesystem, and network; the frontend talks to it exclusively
through typed IPC wrappers.

## Commands (run from repo root)

- `npm run tauri dev` — run the desktop app in dev.
- `npm run check` — `tsc --noEmit` + ESLint + `cargo fmt --check` + `clippy -D warnings`. Run this before considering a change done.
- `cargo test --manifest-path src-tauri/Cargo.toml` — Rust unit tests (all live in `src-tauri/src/core/`).
- `npm run lint` / `npm run format` — ESLint / Prettier on `src/`.

## Architecture

Dependency direction (arrows point "depends on"):

```
UI components ──> lib/ipc.ts ──(invoke)──> commands (lib.rs) ──> core/
```

- **`src-tauri/src/core/`** — pure Rust, **no `tauri` imports**. This is where
  business logic and all unit tests live. Modules: `tree` (file tree + file
  read/write with path-traversal protection), `openrouter` (model fetch + key
  masking), `recent` (recent-roots persistence), `paths` (display-path
  formatting), `errors` (the `AppError` enum).
- **`src-tauri/src/lib.rs`** — entry + `#[tauri::command]` handlers. Handlers
  are thin: validate input, call `core/`, map errors. No business logic here.
- **`src/lib/ipc.ts`** — thin typed wrappers around `invoke` (one function per
  command). UI code never calls raw `invoke` with string command names.
- **`src/App.tsx`** — composition root; owns shared state (e.g. the masked key).
- **`src/components/`** — `ChatPanel`, `SettingsPage`, `FileIcon`.

Full architecture, invariants, and known issues: see the repo memory file
`/memories/repo/architecture.md` (canonical reference for where the project is
going; read it via the memory tool).

## Invariants (do not break)

- `core/` never imports `tauri`. Commands never contain business logic.
- Frontend never calls raw `invoke` outside `lib/ipc.ts`.
- All IPC payloads are `serde`-serializable structs, never ad-hoc
  `serde_json::Value`.
- Every command doing file I/O, network, or keyring access is `async` and runs
  the blocking work via `tauri::async_runtime::spawn_blocking` — a sync command
  freezes the UI.
- Commands return `Result<T, AppError>`; `AppError` (thiserror) serializes to
  its `Display` string for the frontend.
- The backend is the single source of truth for file-type detection (e.g. the
  `is_image` flag on `TreeNode`). The frontend never sniffs extensions.
- Capabilities (`src-tauri/capabilities/*.json`) are default-deny and minimal:
  add a permission only when a feature needs it.

## Gotchas

- **Rust changes require a full app restart** under `tauri dev` (binary
  rebuild); frontend changes hot-reload.
- **`keyring` must be declared with `features = ["windows-native"]`** in
  `Cargo.toml`. v3 has no default features; without the platform backend it
  silently falls back to an in-memory mock store (writes return `Ok` but never
  persist). The OpenRouter key lives in the OS credential manager
  (service `com.mathm.tauri-app`, user `openrouter_key`), never a plaintext file.
- **Root-state staleness**: the root is `Mutex<(Option<PathBuf>, u64)>` where the
  `u64` is a generation counter. Commands capture `(root, gen)` up front and
  re-check `gen` after the blocking work, returning `AppError::StaleRoot` if the
  root changed mid-operation. Preserve this pattern in any new root-scoped command.
- **`csp: null`** in `tauri.conf.json` is the scaffold default — tighten before
  shipping, since the webview will render agent-generated content.
- **`identifier`** (`com.mathm.tauri-app`) is baked into installed apps; changing
  it post-release breaks state/registration.

## Conventions

- Frontend: Solid.js signals/`Show`/`createEffect`; ESLint + Prettier (flat
  config `eslint.config.js`).
- Backend: `cargo fmt` + `clippy -D warnings` must pass.
- Keep the Solid render tree shallow; target narrow signals for frequent updates.
