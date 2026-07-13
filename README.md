# windows-DeviceProfileManager

A CLI and GUI Windows tool to quickly save and apply Monitor + Audio configurations as named profiles.

> **Requires .NET Framework 4.7.2** (ships with Windows 10 1803+ and Windows 11).

---

## What it does

- **Profiles** — save your current monitor layout, resolutions, refresh rates, DPI scaling, HDR state, and audio defaults/volumes as a named profile, then apply any profile with one click.
- **Display control** — set resolution, Hz, DPI, HDR and primary monitor per display. Supports extended desktop and mirror/clone topologies.
- **Audio control** — set the default playback/recording device and volume per profile.
- **GUI** — a simple WinForms window: pick a profile, apply it, or save the current state. No dependencies beyond .NET Framework.
- **CLI** — all features available non-interactively for scripts and shortcuts.

---

## Quick start

```cmd
# Open the GUI (default — no args)
DisplayAudioOrchestrator.exe

# Open the GUI explicitly
DisplayAudioOrchestrator.exe --gui

# Apply a saved profile
DisplayAudioOrchestrator.exe --apply-profile GAMING

# List saved profiles
DisplayAudioOrchestrator.exe --list-profiles

# List detected displays and audio devices
DisplayAudioOrchestrator.exe --list-devices

# Show monitor overlays with GDI names
DisplayAudioOrchestrator.exe --identify

# Save current state as a new profile (opens wizard)
DisplayAudioOrchestrator.exe --save-profile WORK

# Set volume on all active playback devices
DisplayAudioOrchestrator.exe --set-volume-all 50

# Enable debug logging
DisplayAudioOrchestrator.exe --debug
```

---

## First run

1. Run `DisplayAudioOrchestrator.exe` — the GUI opens.
2. Arrange your displays and set your preferred resolution/audio.
3. Click **Save Current as Profile** and give it a name (e.g. `GAMING`, `WORK`, `PRESENTATION`).
4. Repeat for each layout you use.
5. Apply any profile with a double-click or the **Apply** button.

Use `--identify` if you need to see which GDI name (`\\.\DISPLAY1`, etc.) maps to which physical monitor.

---

## Files

| File | Purpose |
|---|---|
| `DisplayAudioOrchestrator.exe` | Main binary — no install needed, run in place |
| `config/devices.json` | Saved profiles (auto-created next to the exe) |
| `logs/orchestrator.log` | Run log |

---

## Notes

- Profiles store hardware identity (adapter LUID + target ID) so they work correctly even when Windows reassigns `DISPLAY1`/`DISPLAY2` numbers after a reboot or topology change.
- Resolution matching is fuzzy: `60 Hz` in a profile matches `59.94 Hz` on the driver; `1080p` matches the closest aspect-ratio-correct mode the monitor reports.

---

## Credits

Inspired by and partially based on [DisplayConfig](https://github.com/MartinGC94/DisplayConfig) by MartinGC94, licensed under MIT.
