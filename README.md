# Kaiser

A Windows desktop app for saving and applying named display + audio profiles. Drag monitors around, enable or disable displays, set resolution and DPI per monitor, and switch between full setups in one click.

Built with [Tauri v2](https://tauri.app/) (Rust backend, React/TypeScript frontend).

---

## Features

- **Profiles** — save your current monitor layout, resolutions, refresh rates, DPI scaling, and audio setup as a named profile, then apply any profile with one click.
- **Display layout** — drag-and-drop canvas to position monitors. Enable or disable individual displays. Edge-snapping for precise alignment.
- **Resolution & DPI** — pick resolution and refresh rate per monitor. Set DPI scaling independently per display.
- **Audio** — set default playback and recording device, volume, and mute state per profile.
- **Safe apply** — a confirmation banner appears after applying a new layout. If you don't confirm within 20 seconds the previous layout is automatically restored.
- **Persistent identity** — monitors are identified by adapter LUID + EDID hash, so profiles survive reboots and driver reinstalls even when Windows reassigns `DISPLAY1`/`DISPLAY2` numbers.

---

## Requirements

- Windows 10 (1803+) or Windows 11
- No runtime dependencies — the MSI installer is self-contained

---

## Install

Download the latest `.msi` from the [Releases](../../releases) page and run it.

---

## Build from source

**Prerequisites:** Rust stable, Node.js 22+, Windows

```sh
git clone --recurse-submodules https://github.com/feuerswut/Kaiser
cd Kaiser/kaiser-app
npm install
npm run tauri build
```

The installer is written to `target/release/bundle/msi/`.

---

## Development

```sh
git clone --recurse-submodules https://github.com/feuerswut/Kaiser
cd Kaiser/kaiser-app
npm install
npm run tauri dev
# or just run dev.bat from the repo root
```

---

## CI / Releases

| Workflow | Trigger | Action |
|---|---|---|
| **CI** | Every push / PR | Type-check frontend, `cargo check` |
| **Release** | `git tag v*` | Full Tauri build, draft GitHub Release with MSI |

---

## Submodules

| Submodule | Purpose |
|---|---|
| [Monarch](https://github.com/feuerswut/Monarch) | Core display management library (CCD API abstractions) |
| [SetResolution](https://github.com/feuerswut/SetResolution) | Resolution/refresh-rate helper |
| [SetDPI](https://github.com/feuerswut/SetDPI) | DPI scaling helper |

---

## License

MIT
