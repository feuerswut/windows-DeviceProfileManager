# windows-DeviceProfileManager

A CLI and GUI Windows PowerShell 5 tool to quickly save and apply Monitor + Audio configurations as named profiles.

> **Requires Windows PowerShell 5.1 — incompatible with PowerShell 7.**
> 
> Requires C# (5.0 in v1)

---

## What it does

- **Profiles** — save your current monitor layout, resolutions, refresh rates, DPI scaling, HDR state, and audio defaults/volumes as a named profile, then apply any profile with one click.
- **Display control** — set resolution, Hz, DPI, HDR and primary monitor per display. Supports extended desktop and mirror/clone topologies.
- **Audio control** — set the default playback/recording device and volume per profile.
- **Device nicknames** — assign stable short names (e.g. `left`, `center`, `laptop`) to physical monitors and audio devices so profiles survive reboots and driver updates.
- **GUI** — a simple WinForms window: pick a profile, apply it, edit it, or save the current state. No dependencies beyond what ships with Windows.
- **CLI** — all features available non-interactively for scripts and shortcuts.

---

## Quick start

```powershell
# Open the GUI (default)
.\DisplayAudioOrchestrator.ps1

# Apply a saved profile
.\DisplayAudioOrchestrator.ps1 -HwProfile GAMING

# List saved profiles
.\DisplayAudioOrchestrator.ps1 -ListProfiles

# List detected devices
.\DisplayAudioOrchestrator.ps1 -ListDevices

# Identify and nickname your monitors/speakers
.\DisplayAudioOrchestrator.ps1 -Identify

# Save current state as a new profile
.\DisplayAudioOrchestrator.ps1 -SaveProfileAs WORK

# Enable debug logging
.\DisplayAudioOrchestrator.ps1 -DebugMode
```

---

## First run

1. Run the script — the GUI opens.
2. Click **Identify Devices** and assign short nicknames to your monitors and audio devices.
3. Arrange your displays and set your preferred resolution/audio, then click **Save Current as Profile**.
4. Repeat for each layout you use (e.g. `GAMING`, `WORK`, `PRESENTATION`).
5. Apply any profile with a double-click or the **Apply** button.

---

## Files

| File | Purpose |
|---|---|
| `DisplayAudioOrchestrator.ps1` | Main script — self-contained, no install needed |
| `config/devices.json` | Saved nicknames and profiles (auto-created) |
| `logs/orchestrator.log` | Run log |
| `README.ai.md` | Developer/AI reference — section map and architecture notes |

---

## Notes

- Profiles store hardware identity (adapter LUID + target ID, EDID) so they work correctly even when Windows reassigns `DISPLAY1`/`DISPLAY2` numbers after a reboot or topology change.
- Resolution matching is fuzzy: `60 Hz` in a profile matches `59.94 Hz` on the driver; `1080p` matches the closest aspect-ratio-correct mode the monitor reports.
- The `(don't change)` option in the profile editor leaves a display's active state, resolution, or audio default untouched when applying the profile.

---

## Credits

Inspired by and partially based on [DisplayConfig](https://github.com/MartinGC94/DisplayConfig) by MartinGC94, licensed under MIT.
