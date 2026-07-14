# AGENTS.md — Kaiser codebase guide

Read this before editing any file. It describes the project structure, key design decisions, and important invariants that are non-obvious from the code alone.

---

## What Kaiser is

A Windows desktop app (Tauri v2) for saving and applying named display + audio profiles. The Rust backend talks directly to the Windows CCD API (`QueryDisplayConfig` / `SetDisplayConfig`) and COM audio APIs. The frontend is React + TypeScript.

The binary also has a headless CLI mode (no Tauri window) for scripting.

---

## Repository layout

```
Kaiser/
├── kaiser-core/          Rust library — all Windows backend logic
├── kaiser-app/
│   ├── src/              React/TypeScript frontend
│   └── src-tauri/        Tauri app shell (commands, state, CLI)
├── Monarch/              Git submodule — display manager framework
├── SetResolution/        Git submodule — resolution/Hz helper exe
├── SetDPI/               Git submodule — DPI scaling helper exe
└── .github/workflows/    CI (ci.yml) and release (release.yml)
```

**Cargo workspace members:** `kaiser-core` and `kaiser-app/src-tauri`. `Monarch` is a path dependency (`../Monarch`), not a workspace member.

---

## Crate responsibilities

### `kaiser-core` (library)

Four modules:

| Module | Responsibility |
|---|---|
| `backend` | CCD topology query, layout apply, snapshot cache — the hard part |
| `audio` | COM-based audio device enumeration, volume, mute, default device |
| `resolution` | `EnumDisplaySettings` / `ChangeDisplaySettingsEx` for res+Hz |
| `profile` | Config file schema, serialization, `KaiserConfigStore` |

Everything is gated `#[cfg(target_os = "windows")]`. Non-Windows stubs return `ManagerError::Backend("not supported")`.

### `kaiser-app/src-tauri` (Tauri app)

Four files:

| File | Responsibility |
|---|---|
| `main.rs` | Entry point — CLI args → headless CLI, otherwise Tauri GUI |
| `lib.rs` | Tauri builder, invoke handler, `env_logger` init |
| `state.rs` | `AppState` — `MonarchDisplayManager`, `KaiserBackend`, `AudioManager` |
| `commands.rs` | All 23 `#[tauri::command]` functions |
| `cli.rs` | Headless `--apply-profile`, `--list-*` etc. |

### `Monarch` submodule

Provides the generic display manager framework that Kaiser plugs into:
- `DisplayBackend` trait — Kaiser implements this via `SharedKaiserBackend`
- `ConfigStore` trait — Kaiser implements this via `KaiserConfigStore`
- `MonarchDisplayManager<B, S>` — manages profile CRUD, layout confirmation/rollback, toggle logic
- Core types: `DisplayId`, `DisplayInfo`, `Layout`, `OutputConfig`, `Position`, `Resolution`

**Do not edit Monarch files.** It is a separate repo and a read-only submodule here.

---

## Key types

### `TopologySnapshot` (`kaiser-core/src/backend/win32_types.rs`)

```rust
pub struct TopologySnapshot {
    pub raw: RawTopologySnapshot,   // DISPLAYCONFIG_PATH_INFO / MODE_INFO arrays
    pub layout: Layout,             // parsed: positions, resolutions, enabled flags
    pub displays: Vec<DisplayInfo>, // parsed: names, IDs, resolutions
    pub gdi_names: HashMap<(u64, u32), String>, // (adapter_luid, target_id) → "\\.\DISPLAYn"
}
```

The `raw` field is what gets passed to `SetDisplayConfig`. `layout` and `displays` are derived from it and used by the frontend. The two must always be in sync — never construct a `TopologySnapshot` by mixing `raw` from one query and `layout` from another, except through `merge_snapshot_for_cache`.

### `DisplayId` (from `Monarch`)

```rust
pub struct DisplayId { pub adapter_luid: u64, pub target_id: u32, pub edid_hash: Option<u64> }
```

**JavaScript JSON loses u64 precision.** Both `adapter_luid` and `edid_hash` can exceed `Number.MAX_SAFE_INTEGER`. Mitigation:
- `edid_hash` round-trips as lossy — every command that receives a `DisplayId` from the frontend calls `resolve_display_id` or `fix_layout_display_ids` (in `commands.rs`) to re-attach the correct `edid_hash` from the live backend state before using the ID.
- The `gdi_names` map in `SnapshotDto` uses `"LUID:TID"` string keys (not objects) for the same reason.
- `displayKey(id)` in the frontend = `"${adapter_luid}:${target_id}"` — used as React keys and map keys everywhere.

### `KaiserProfile` / `KaiserConfigStore` (`kaiser-core/src/profile.rs`)

Kaiser extends Monarch's `Profile` with:
- `audio: Vec<AudioSetting>` — pattern-match rules applied at profile load time
- `dpi_scales: HashMap<String, u32>` — key is `"adapter_luid:target_id"`, value is Windows DPI percent (100/125/150/…)

`KaiserConfigStore` implements `monarch::ConfigStore`. It reads/writes `%APPDATA%\Kaiser\config.json` on **every operation** — no in-memory cache. `load()` strips the Kaiser extensions and gives Monarch the plain `Layout`; `save()` merges Monarch's updated profile list back in, preserving audio and DPI data.

---

## Backend design — `KaiserBackend` (`kaiser-core/src/backend/topology.rs`)

### Snapshot cache

`BackendCache` holds `last_snapshot: Option<TopologySnapshot>`. It is lazily populated by `ensure_snapshot()` on first use, then kept current after every `apply_layout`. The snapshot contains both the parsed layout and the raw Win32 path/mode arrays needed to call `SetDisplayConfig`.

`refresh_snapshot()` is called after every apply. It runs `merge_snapshot_for_cache` before storing:

```
if previous.raw.paths.len() > fresh.raw.paths.len()
   AND previous.raw covers all active outputs in fresh.layout
→ keep previous.raw, take fresh.layout/displays
```

**Why:** `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)` drops disabled monitors from the result. After you disable a monitor, the raw snapshot no longer contains its path. When re-enabling, there would be no valid path entry to give `SetDisplayConfig`. The merge preserves the larger raw snapshot (with valid mode indices for all paths) so re-enabling works without needing `QDC_ALL_PATHS`.

### `apply_layout` fallback chain

Three tiers, each only reached if the previous fails with `ERROR_INVALID_PARAMETER` (error 87):

1. **Normal apply** — use the merged (cached) snapshot directly as the `SetDisplayConfig` base.
2. **Inactive-path attach** (`try_attach_inactive_for_layout` in `apply.rs`) — queries `QDC_ALL_PATHS`, finds the specific inactive path, appends it to the active paths with `modeInfoIdx = 0xFFFFFFFF`, calls `SetDisplayConfig` with `SDC_ALLOW_CHANGES`. Only adds the needed path(s), not all 116 inactive paths (which causes error 87). Windows picks a mode automatically.
3. **Force extend** — `SetDisplayConfig(SDC_TOPOLOGY_EXTEND)` or `DisplaySwitch.exe /extend`, then 700ms sleep, then retry with fresh `query_active_topology`.

Tiers 2 and 3 are logged at `ERROR` level so you can see in the logs which path was taken.

### Persisted snapshot (`%APPDATA%\Kaiser\topology_snapshot.json`)

Written after every `apply_layout`. Contains the active displays and layout. On startup, `list_displays` and `get_layout` merge this into the live topology to show previously-seen-but-now-inactive monitors as "off" so users can re-enable them from the UI.

`persist_snapshot` also merges: previously-active displays/outputs that aren't in the current snapshot are appended with `is_active=false` / `enabled=false`. This is the only cross-restart memory of disabled monitors.

### Color/gamma preservation

`BackendCache` also holds `sdr_gamma_cache` (per-display gamma ramps) and `last_color_state_signature` (detects HDR toggles). After every topology change, `apply_layout_against_snapshot` calls the Win32 WCS API to reload color calibration and restores saved gamma ramps per display.

---

## Frontend design (`kaiser-app/src/`)

### Data flow

```
App.tsx (polls get_snapshot every 3s)
  → snapshot: SnapshotDto
      ├── DisplaysTab — layout drag canvas, per-display controls
      ├── ProfilesTab — profile list, apply/save/edit
      └── AudioTab    — live audio device controls
```

All backend calls go through `api.ts` which wraps `invoke` with error logging to both browser console and the Rust `log` crate (via `frontend_log`).

### `LayoutCanvas` (`DisplaysTab.tsx`)

- 6000×6000 virtual canvas. Monitors are absolutely positioned within it at `position * scale`.
- Scale = fit the active-monitor bounding box with **30% margin** (`outerSize * 0.7 / bounds`), capped at 1.0.
- Canvas pan: background drag moves the `offset` translation.
- Monitor drag: pointer events on individual monitor divs, with `edgeSnap()` snapping to nearest edges of other enabled monitors within 60 virtual pixels (axes independent).
- Snap-back: on pointer-up, if less than 75% of the bounding box is visible (`visArea / totalArea < 0.75`), the canvas animates 50% of the way back toward center. Uses a `snapAnimating` flag to conditionally add a CSS `transition` — disabled during drag to keep dragging instant.
- `normalizeLayout()` shifts the primary monitor to (0, 0) before sending to the backend (Windows CCD requirement: primary must be at origin).

### Layout draft state

`DisplaysTab` keeps a local `draftLayout` that diverges from the server state while the user drags. Changes are only sent to the backend when the user clicks **Apply Layout**. The `ProfilesTab` edit panel works the same way with its own local `layout` state.

### Confirmation banner

When `snapshot.pending_confirmation` is true, a banner overlays the UI. It shows a countdown re-synced from the server every poll cycle but ticked locally at 100ms for smooth display. "Confirm" calls `confirm_layout`; "Revert" calls `revert_layout`. The revert timeout is **20 seconds** (set in `state.rs`).

---

## `commands.rs` — important invariants

- **Always call `resolve_display_id` / `fix_layout_display_ids`** before using any `DisplayId` received from the frontend. The frontend's `edid_hash` is likely corrupted by JSON `Number` precision loss.
- **`toggle_display`** uses `manager.toggle_output()` (Monarch), not `apply_layout` directly. Monarch handles the disabled-monitor bookkeeping.
- **`save_profile`** captures the current live DPI values at save time (reads from Win32). It does not use any cached value.
- **`apply_profile`** applies display layout first, then audio (match by `pattern` substring against `AudioDevice.name`, case-insensitive), then DPI per monitor.
- **`set_display_mode` / `set_display_mode_for_id`** call `backend.invalidate_snapshot()` after changing resolution, because the raw path/mode data changes and the cached snapshot would be stale.
- **`frontend_log`** routes `console.error` from the frontend into the Rust `log` crate at the appropriate level. Errors appear in the same log stream as backend errors.

---

## CLI mode

If the binary is launched with any arguments, `main.rs` calls `cli::run()` and exits before Tauri initialises. CLI mode constructs its own `KaiserBackend` and `KaiserConfigStore` instances (no shared `AppState`). `--apply-profile` calls `confirm_current_layout()` immediately after apply (no confirmation timer).

---

## Conventions

- **Logging:** `log::info!` / `log::warn!` / `log::error!` (Rust). Frontend errors go via `frontend_log`. `env_logger` defaults to `info` level.
- **Display identity:** always `(adapter_luid, target_id)` as the stable hardware key. Never use GDI name (`\\.\DISPLAY1`) as an identity — it reassigns across reboots. GDI names are resolved lazily from the backend cache when needed for Win32 calls.
- **Millihertz vs Hz:** the CCD API uses millihertz (`refresh_rate_mhz` in `OutputConfig` / `DisplayInfo`). `EnumDisplaySettings` uses integer Hz (`refresh_rate_hz` in `DisplayMode`). Do not mix them. The UI converts: `Math.round(mhz / 1000)`.
- **No `QDC_ALL_PATHS` as the primary path:** using all 116+ inactive paths as a `SetDisplayConfig` base causes error 87 (invalid mode indices). The merge-snapshot approach is the primary enable-inactive strategy; `QDC_ALL_PATHS` is only used in the tier-2 fallback to find and append a single inactive path.
- **`cargo check --workspace`** is the CI gate. The release build runs `tauri-action` on `windows-latest` and produces an MSI. Tag `v*` to trigger a draft release.
