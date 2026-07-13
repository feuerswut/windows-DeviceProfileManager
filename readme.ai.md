# DisplayAudioOrchestrator — AI Reference

Read this before editing any C# file. The project converts the original PS1 script
(now at `.old/DisplayAudioOrchestrator.ps1`) into a standalone .NET 4.7.2 executable.

**CRITICAL: The `SetResolution/` folder is a git submodule and is READ-ONLY.
IT MUST NOT BE EDITED UNDER ANY CIRCUMSTANCE.**

---

## Solution layout

```
DisplayAudioOrchestrator.sln
  SetResolution/                   ← READ-ONLY git submodule (https://github.com/feuerswud/SetResolution)
  SetResolutionAdapters/           ← class lib: P/Invoke bridges
    DisplayManagerAdapter.cs
  SetResolutionReplacements/       ← class lib: outright replacements (currently placeholder only)
    Placeholder.cs
  DisplayAudioOrchestrator/        ← main exe project (net472, WinForms)
    Program.cs
    OrchestratorLogger.cs
    OrchestratorCommandLineParser.cs
    OrchestratorProcessor.cs
    CCD/
      DisplayConfigNative.cs       ← CCD structs + P/Invoke (ported from PS1 DisplayNative Add-Type)
      DisplayConfigManager.cs      ← GetAllDisplayInfo, ConfigureTopology, DPI, HDR
    Audio/
      AudioNative.cs               ← COM interfaces (ported from PS1 AudioNative Add-Type)
      AudioManager.cs              ← GetAllDevices, SetVolume, SetDefaultEndpoint, FindByPattern
    Orchestrator/
      OrchestratorState.cs         ← data model: DeviceState, OrchestratorProfile, etc.
      StateStore.cs                ← load/save config/devices.json via Newtonsoft.Json
      NicknameRegistry.cs          ← register/resolve nicknames for displays and audio
      ProfileManager.cs            ← full apply pipeline with verification
      ProfileNotAppliedException.cs
    GUI/
      ProfileSwitcherForm.cs       ← main window (default entry point, no-args)
      DeviceIdentificationForm.cs  ← tabbed wizard: register display/audio nicknames
      ResolutionPickerForm.cs      ← per-display resolution+Hz picker
      MonitorOverlayForm.cs        ← fullscreen flash overlay (like Windows Identify)
  .old/
    DisplayAudioOrchestrator.ps1   ← original script, reference only
```

---

## Architecture: three-layer display abstraction

```
Physical Monitor  <->  GDI short name (DISPLAY1)  <->  Nickname  <->  Profile
```

| Layer | Identified by | Stored in |
|---|---|---|
| Physical | GDI short name (DISPLAY1) — primary stable key; FriendlyName substring — fallback | resolved live |
| Nickname | stable abstract name chosen by user | `devices.json .displays.<nick>` |
| Profile | nickname references + settings | `devices.json .profiles.<name>` |

**Design decisions:**
- GDI short name (DISPLAY1, DISPLAY2) from `EnumDisplayDevices` is the primary stable identifier.
- FriendlyName substring is a fallback only, used when GDI name changes (GPU driver reinstall, port swap).
- No LUID/targetId/EDID matching — reduces complexity at the cost of needing re-registration on GPU port changes.
- `NicknameRegistry.ResolveDisplay()` is the single resolution point.

---

## File map

| File | Key types / methods |
|---|---|
| `SetResolutionAdapters/DisplayManagerAdapter.cs` | `DisplayDeviceInfo`, `DisplayModeInfo`, `GetAllDisplayDevices()`, `GetDisplayModes()`, `GetCurrentMode()`, `SetDisplayMode()`, `FindBestMode()` |
| `CCD/DisplayConfigNative.cs` | All CCD P/Invoke structs (DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_MODE_INFO, DpiConfigGet/Set, AdvancedColorInfo2, SetHdrState, LUID, etc.) and P/Invoke declarations |
| `CCD/DisplayConfigManager.cs` | `DisplayInfo`, `GetAllDisplayInfo()`, `ConfigureTopology()`, `GetDpiPercent()`, `SetDpiPercent()`, `GetHdrEnabled()`, `SetHdrEnabled()` |
| `Audio/AudioNative.cs` | COM interfaces: `IMMDeviceEnumerator`, `IMMDevice`, `IAudioEndpointVolume`, `IPolicyConfig`, `PROPERTYKEY`, `PROPVARIANT`, `AudioGuids` |
| `Audio/AudioManager.cs` | `AudioDeviceInfo`, `GetAllDevices()`, `GetVolume()`, `SetVolume()`, `GetMute()`, `SetMute()`, `SetDefaultEndpoint()`, `FindByPattern()` |
| `Orchestrator/OrchestratorState.cs` | `DisplayNickname`, `AudioNickname`, `ProfileDisplay`, `ProfileAudio`, `StartProcess`, `OrchestratorProfile`, `DeviceState` |
| `Orchestrator/StateStore.cs` | `Load()`, `Save()`, `ConfigPath` — `config/devices.json` next to exe |
| `Orchestrator/NicknameRegistry.cs` | `RegisterDisplay()`, `RemoveDisplay()`, `ResolveDisplay()`, `RegisterAudio()`, `RemoveAudio()`, `ResolveAudio()` |
| `Orchestrator/ProfileManager.cs` | `Apply()` — full pipeline with atomic calls and verification |
| `OrchestratorLogger.cs` | `Log()`, `Debug()`, `DebugMode`, `LogEvent` event |
| `OrchestratorCommandLineParser.cs` | `OrchestratorOptions`, `Parse()`, `PrintHelp()` |
| `OrchestratorProcessor.cs` | `Process()` — CLI dispatch |
| `GUI/MonitorOverlayForm.cs` | `MonitorOverlayForm.ShowOverlays()` — static, shows all active monitor overlays |
| `GUI/ResolutionPickerForm.cs` | `ResolutionPickerForm`, `ResolutionResult` |
| `GUI/DeviceIdentificationForm.cs` | `DeviceIdentificationForm` — tabbed nickname registration wizard |
| `GUI/ProfileSwitcherForm.cs` | `ProfileSwitcherForm` — main window |
| `Program.cs` | `Main()` — arg-less → GUI, otherwise CLI |

---

## Data flow

### Profile apply (`ProfileManager.Apply`)
```
devices.json
  → Step 1: ConfigureTopology (one shot enable/disable via CCD)
  → Step 2: SetPrimary (separate CCD call)
  → Step 3: Per-display:  SetDisplayMode (res+hz) → verify+retry ×2
                          → SetDpiPercent
                          → SetHdrEnabled
  → Step 4: Audio:        SetDefaultEndpoint → SetVolume → SetMute
  → Step 5: StartProcesses
  → Verify: re-enumerate active displays, throw ProfileNotAppliedException if mismatch
```

### Display nickname resolution (`NicknameRegistry.ResolveDisplay`)
```
Nickname → state.Displays[nick].GdiName     → exact match against live DisplayInfo.GdiShortName
         → state.Displays[nick].FriendlyName → substring match against live DisplayInfo.FriendlyName
         → null (caller handles unresolved)
```

### Logging
```
OrchestratorLogger.Log(msg, level)
  → always Console.WriteLine(msg)
  → fires LogEvent(msg, level)
      ↳ ProfileSwitcherForm subscribes → appends WARN+ERROR to output TextBox
```

---

## State file schema (`config/devices.json`)

```jsonc
{
  "displays": {
    "<nickname>": {
      "friendlyName": "HP E27q G4",     // partial name for fallback matching
      "gdiName":      "DISPLAY1",        // primary stable key from EnumDisplayDevices
      "notes":        ""
    }
  },
  "audio": {
    "<nickname>": {
      "pattern":  "substring of FriendlyName",
      "type":     "Playback|Recording",
      "deviceId": "IMMDevice endpoint ID",   // stored for diagnostics only
      "notes":    ""
    }
  },
  "profiles": {
    "<name>": {
      "displays": [
        {
          "nickname":   "<nick>",
          "active":     true,
          "primary":    false,
          "width":      1920,
          "height":     1080,
          "hz":         60,
          "dpiPercent": 125,      // null = don't touch
          "hdr":        false,    // null = don't touch
          "rotation":   null,
          "mirrorOf":   null      // null = extend; "<nickname>" = clone
        }
      ],
      "audio": [
        { "nickname": "<nick>", "setDefault": true, "volume": 50, "mute": false }
      ],
      "startProcesses": [
        { "path": "C:\\foo.exe", "args": "--flag", "asAdmin": false }
      ]
    }
  }
}
```

---

## CLI switches

```
DisplayAudioOrchestrator.exe                    → open GUI (ProfileSwitcherForm)
  --gui                                         → open GUI explicitly
  --apply-profile <name>                        → apply profile, exit 0 on success / exit 1 on fail
  --save-profile  <name>                        → save current state as profile
  --list-profiles                               → print all profiles to console
  --list-devices                                → print all displays + audio to console
  --identify                                    → flash MonitorOverlayForm on all active monitors
  --set-volume-all <0-100>                      → set volume on all active playback devices
  --debug                                       → enable verbose [DBG] output
  --help                                        → show help text
```

---

## DPI values
Constrained to `DisplayConfigFlags.DpiValues` = `{ 100,125,150,175,200,225,250,300,350,400,450,500 }`.
`SetDpiPercent` picks the nearest entry.

## Conventions
- **No FriendlyName-only matching for profiles** — GDI name is the key; FriendlyName is fallback.
- **Atomic calls**: `SetDisplayMode` (res+hz together) is separate from `SetDpiPercent` and `SetHdrEnabled`.
- **All logging → Console always**; GUI output box receives WARN+ERROR via `LogEvent` event.
- **Throw `ProfileNotAppliedException`** if post-apply verification detects any mismatch.
- The `SetResolution` submodule is NOT referenced as a project — it is a standalone exe in its own folder.
