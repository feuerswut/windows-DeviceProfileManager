# DisplayOrchestrator.ps1 — AI Reference

Read this before editing the script. The file is ~2500 lines; use the grep patterns below to jump directly to relevant code rather than reading top-to-bottom.

---

## Architecture: three-layer display abstraction

```
Physical Monitor  <->  hardware identity  <->  Nickname  <->  Profile
```

| Layer | Identified by | Stored in |
|---|---|---|
| Physical | adapterLUID+targetId (runtime), edidManufactureId+edidProductCodeId+connectorInstance (stable) | resolved live |
| Nickname | stable abstract name chosen by user | `devices.json .displays.<nick>` |
| Profile | nickname references + settings | `devices.json .profiles.<name>` |

**Design decisions:**
- No FriendlyName pattern matching — patterns are ambiguous for identical monitor models.
- Hardware identity has two tiers: GPU port (LUID+TargetId, session-stable) then EDID (stable across reboots on same port type).
- The `@ADAPTER:HIGH-LOW:TARGETID` key format is accepted by all `DisplayNative` C# methods, enabling unambiguous routing to a specific physical port.
- `Resolve-DisplayPathForNickname` is the single resolution point — all profile-apply and save-profile code uses it.

---

## Section map

Every logical block is wrapped with `#region SECTION: <Name>` / `#endregion SECTION: <Name>`.

| Section name | What lives there |
|---|---|
| `Config_Paths` | `$Script:RootDir`, `$Script:ConfigDir`, `$Script:StateFile`, `$Script:LogFile` |
| `Logging` | `Write-Log`, `Write-Section`, `Show-Help` |
| `Win32_Types — DisplayNative` | C# `Add-Type` block: CCD structs/P-Invoke (QueryDisplayConfig, SetDisplayConfig, ChangeDisplaySettingsExW, DPI, HDR) |
| `Win32_Types — AudioNative` | C# `Add-Type` block: Core Audio COM interfaces (IMMDeviceEnumerator, IAudioEndpointVolume, IPolicyConfig) |
| `State_Store` | `Get-State` / `Save-State` — reads/writes `config/devices.json` |
| `Nickname_Registry` | `Register-DisplayNickname`, `Register-AudioNickname`, `Remove-Nickname`, `Get-DisplayNicknameFor`, `Get-AudioNicknameFor`, duplicate-check helpers |
| `Display_CCD_Low` | `Get-AllDisplayPaths` (raw path enumeration, DPI/HDR/position per path), `Get-ActiveDisplayNames`, `Get-DisplayDeviceGuid` |
| `Display_CCD_Mid` | Pattern-based enable/disable/primary: `Enable-DisplayByPattern`, `Disable-DisplaysExcept`, `Set-DisplayPrimaryByPattern`, `Invoke-ExtendUntilActive`, `Enable-AllKnownDisplays` |
| `Display_Resolution` | `Get-DisplayModesForPattern`, `Set-DisplayModeForPattern`, `Get-GdiDeviceNameForPattern`, preset tables `$Script:ResolutionPresets` / `$Script:FrameRatePresets` |
| `Display_DPI` | `Get-DisplayDpiInfo`, `Set-DisplayDpi` — per-monitor DPI % via CCD |
| `Display_HDR` | `Get-DisplayHdr`, `Set-DisplayHdr` — per-display HDR toggle |
| `Audio_Mid` | `Get-AllAudioDevices`, `Resolve-AudioDeviceByPattern`, `Set-AudioDeviceDefaultByPattern`, `Set-AudioDeviceVolumeByPattern`, `Set-AllAudioVolume` |
| `Profile_Management` | `Save-Profile`, `Save-ProfileInteractive`, `Invoke-Profile`, `Get-Profiles`, `Remove-Profile` — full profile apply pipeline (5 steps) |
| `Identification_Wizard` | `Start-DeviceIdentificationWizard` — console wizard for assigning nicknames |
| `Console_Resolution_Picker` | `Read-ResolutionForDisplay`, `Read-MenuChoice` — console prompts used by `Save-ProfileInteractive` |
| `GUI_Resolution_Picker` | `Show-ResolutionPickerDialog`, `Show-ResolutionPickerGuiForAllDisplays` — WinForms resolution picker popup |
| `GUI_Identification_Wizard` | `Show-DeviceIdentificationGui` — WinForms device identification wizard (tab per device type, inline nickname dialog) |
| `GUI_Profile_Switcher` | `Show-ProfileSwitcherGui` — main WinForms window; default entry point |
| `CLI_Entry_Point` | `param(...)` dispatch switch at the bottom of the file |

---

## Grep patterns

```powershell
# Jump to any section
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern '#region SECTION:'

# Find a specific section
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern '#region SECTION: Profile_Management'

# Find a function
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern '^function Invoke-Profile'

# Find all public functions
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern '^function '

# Find all C# P/Invoke signatures
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern 'DllImport'

# Find profile schema comment
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern 'Profile schema'

# Find all Write-Log calls (understand error/warn paths)
Select-String -Path .\DisplayOrchestrator.ps1 -Pattern "Write-Log.*-Level (ERROR|WARN)"
```

---

## Key data flow

### Profile apply (`Invoke-Profile`)
```
devices.json (.profiles.<Name>)
  → Step 1: Configure topology (one shot) [DisplayNative]::ConfigureTopology
               extend: each display gets a unique GPU source (no accidental mirror)
               clone:  displays with mirrorOf share the same GPU source
  → Step 2: Set primary                   Set-DisplayPrimaryByPattern
  → Step 3: Resolution / DPI / HDR       Set-DisplayModeForPattern
                                          Set-DisplayDpi
                                          Set-DisplayHdr
  → Step 4: Audio default + volume       Set-AudioDeviceDefaultByPattern
                                          Set-AudioDeviceVolumeByPattern
  → Post:   Start processes              Invoke-ProfilePostActions
```

### Profile save (`Save-ProfileInteractive`)
```
Get-AllDisplayPaths          → snapshot active resolution/DPI/HDR/rotation per nick
Read-ResolutionForDisplay    → optional console override (Console mode)
Show-ResolutionPickerDialog  → optional GUI override (Gui mode)
Get-AllAudioDevices          → snapshot IsDefault + volume per nick
Save-Profile                 → write to devices.json
```

### Display path resolution chain
```
Nickname
  → Resolve-DisplayPathForNickname    hardware-identity lookup (Tier 1: LUID+TargetId, Tier 2: EDID)
  → path.GdiDeviceName               direct GDI device name — no FriendlyName involved
  → Set-DisplayModeForGdiDevice       EnumDisplaySettings / ChangeDisplaySettingsEx
  → [DisplayNative]::SetDpiPercent    DisplayConfigSetDeviceInfo DI_SET_DPI_SCALE
  → [DisplayNative]::SetHdrEnabled    DisplayConfigSetDeviceInfo DI_SET_HDR_STATE
ConfigureTopology / EnableMonitorByName / SetPrimaryByName
  → Get-HardwareKeyForNickname        "@ADAPTER:HIGH-LOW:TARGETID" key
  → [DisplayNative] C# methods        MatchPath() routes by LUID+TargetId, not FriendlyName
```

---

## State file schema (`config/devices.json`)

```jsonc
{
  "displays": {
    "<nickname>": {
      "notes":             "",
      "friendlyName":      "HP E27q G4",        // display only, not used for matching
      "adapterLUID":       "0-12345",            // Tier 1 match key (runtime GPU port)
      "targetId":          "42",                 // Tier 1 match key
      "connectorInstance": 4,                    // Tier 2 match key (0,1,2... per connector type)
      "edidManufactureId": 8785,                 // Tier 2 match key (EDID vendor ID)
      "edidProductCodeId": 12345,                // Tier 2 match key (EDID product code)
      "serial":            "ABC123"              // Tier 2 refinement (optional)
    }
  },
  "audio": {
    "<nickname>": {
      "pattern":  "substring of FriendlyName",
      "type":     "Playback|Recording",
      "notes":    "",
      "deviceId": "IMMDevice endpoint ID"
    }
  },
  "profiles": {
    "<name>": {
      "displays": [
        {
          "nickname":    "<nick>",
          "active":      true,
          "primary":     false,
          "width":       1920,
          "height":      1080,
          "refreshRate": 60,
          "dpiPercent":  125,       // null = don't touch; legacy field: dpiScaling (same semantics)
          "hdr":         false,     // null = don't touch
          "rotation":    0,         // 0=landscape 1=portrait 2=flip-landscape 3=flip-portrait
          "mirrorOf":   null        // null = extend; "<nickname>" = clone this display onto that nick's source
        }
      ],
      "audio": [
        { "nickname": "<nick>", "setDefault": true, "volume": 50 }
      ],
      "startProcesses": [
        { "path": "C:\\foo.exe", "args": "--flag", "asAdmin": false }
      ]
    }
  }
}
```

---

## Conventions

- **Nicknames** are the stable internal key used in profiles. Physical monitors are resolved from nicknames via hardware identity (adapterLUID+targetId first, EDID second) — never by FriendlyName pattern.
- **Hardware keys** (`@ADAPTER:HIGH-LOW:TARGETID`) are accepted by all `DisplayNative` C# methods. `Get-HardwareKeyForNickname` produces them; `MatchPath()` inside C# consumes them.
- **DPI values** are constrained to `[DisplayNative]::DpiValues` = `{ 100,125,150,175,200,225,250,300,350,400,450,500 }`. Any other value will fail silently in `SetDpiPercent`.
- **`dpiScaling`** is the legacy field name for `dpiPercent` in old profile entries — `Invoke-Profile` handles both transparently.
- **Audio deduplication**: `Get-AllAudioDevices` returns every endpoint including inactive/virtual ones. Both wizards group by `FriendlyName|Type` and present the best representative (default > active). The `DuplicateCount` property is attached by the wizard; `AudioNative.GetAllDevices()` does not set it.
- **C# types** are guarded with `if (-not ([System.Management.Automation.PSTypeName]'DisplayNative').Type)` — safe to dot-source or re-import.
- All display operations that call `SetDisplayConfig` or `ChangeDisplaySettingsEx` retry with `Wait-DisplayState` / loop up to `$MaxAttempts` (default 5).
