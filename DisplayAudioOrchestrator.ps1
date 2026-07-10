#Requires -Version 5.1
# AI: Read README.ai.md in this directory before editing. It has the section map, grep patterns,
#     data-flow diagrams, and state-file schema. Do not read this file top-to-bottom.
<#
.SYNOPSIS
    Unified Display & Audio Orchestrator — self-contained, no external dependencies.
.DESCRIPTION
    All features available from console and GUI. Default launch opens GUI.
    Replaces AudioDeviceCmdlets module with inline Core Audio COM.
    Replaces SetResolution.exe with inline EnumDisplaySettings/ChangeDisplaySettings.
    Adds: per-monitor DPI, HDR toggle, display positions, rotation.
.PARAMETER Gui
    Launch the GUI (default when no other switch given).
.PARAMETER Profile
    Apply a named profile non-interactively.
.PARAMETER ListProfiles
    Print all profile names.
.PARAMETER ListDevices
    Print all detected displays and audio devices.
.PARAMETER Identify
    Run the interactive device-nickname wizard.
.PARAMETER SetVolumeAll
    Set volume (0-100) on every nicknamed audio device.
.PARAMETER SaveProfileAs
    Capture current live state as a new named profile.
.PARAMETER Help
    Print help.
.EXAMPLE
    .\DisplayOrchestrator.ps1
.EXAMPLE
    .\DisplayOrchestrator.ps1 -Profile STANDARD
.EXAMPLE
    .\DisplayOrchestrator.ps1 -ListDevices
#>

[CmdletBinding(DefaultParameterSetName = 'Gui')]
param(
    [Parameter(ParameterSetName = 'Gui')]    [switch]$Gui,
    [Parameter(ParameterSetName = 'ApplyProfile', Mandatory)][string]$Profile,
    [Parameter(ParameterSetName = 'ListProfiles')]  [switch]$ListProfiles,
    [Parameter(ParameterSetName = 'ListDevices')]   [switch]$ListDevices,
    [Parameter(ParameterSetName = 'Identify')]      [switch]$Identify,
    [Parameter(ParameterSetName = 'SetVolumeAll', Mandatory)][ValidateRange(0,100)][int]$SetVolumeAll,
    [Parameter(ParameterSetName = 'SaveProfile', Mandatory)][string]$SaveProfileAs,
    [Parameter(ParameterSetName = 'Help')][Alias('h')][switch]$Help,
    [Parameter(ValueFromRemainingArguments, DontShow)][string[]]$RemainingArgs
)

# =============================================================================
#region SECTION: Config_Paths
# =============================================================================

$Script:RootDir   = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$Script:ConfigDir = Join-Path $RootDir 'config'
$Script:StateFile = Join-Path $ConfigDir 'devices.json'
$Script:LogFile   = Join-Path $ConfigDir 'orchestrator.log'

if (-not (Test-Path $ConfigDir)) { New-Item -Path $ConfigDir -ItemType Directory -Force | Out-Null }

#endregion SECTION: Config_Paths

# =============================================================================
#region SECTION: Logging
# =============================================================================

function Write-Log {
    param(
        [Parameter(Mandatory)][string]$Message,
        [ValidateSet('INFO','OK','WARN','ERROR','STEP')][string]$Level = 'INFO'
    )
    $color  = switch ($Level) { 'OK' {'Green'} 'WARN' {'Yellow'} 'ERROR' {'Red'} 'STEP' {'Cyan'} default {'Gray'} }
    $prefix = switch ($Level) { 'STEP' {'==>'} default {"[$Level]"} }
    Write-Host "$prefix $Message" -ForegroundColor $color
    try {
        $line = "{0:yyyy-MM-dd HH:mm:ss} [{1}] {2}" -f (Get-Date), $Level, $Message
        Add-Content -Path $Script:LogFile -Value $line -ErrorAction Stop
    } catch {}
}

function Write-Section {
    param([string]$Title)
    Write-Host ""
    Write-Host ("=" * 60) -ForegroundColor Cyan
    Write-Host "  $Title"   -ForegroundColor Cyan
    Write-Host ("=" * 60) -ForegroundColor Cyan
}

function Show-Tip { Write-Host "Tip: run with -h or --help to see all commands." -ForegroundColor DarkGray }

function Show-Help {
    Get-Help -Name $PSCommandPath -Detailed | Out-Host
    Write-Section "Quick Reference"
    @(
        "  .\DisplayOrchestrator.ps1                      Launch GUI (default)"
        "  .\DisplayOrchestrator.ps1 -Profile <name>      Apply saved profile"
        "  .\DisplayOrchestrator.ps1 -ListProfiles        List profile names"
        "  .\DisplayOrchestrator.ps1 -ListDevices         Show all devices + nicknames"
        "  .\DisplayOrchestrator.ps1 -Identify            Device nickname wizard"
        "  .\DisplayOrchestrator.ps1 -SetVolumeAll <0-100> Set volume on all audio devices"
        "  .\DisplayOrchestrator.ps1 -SaveProfileAs <name> Save current state as profile"
        "  .\DisplayOrchestrator.ps1 -h                   This help"
    ) | ForEach-Object { Write-Host $_ -ForegroundColor Gray }
    Write-Host ""
}

#endregion SECTION: Logging

# =============================================================================
#region SECTION: Win32_Types — DisplayNative (CCD + Resolution + DPI + HDR)
# =============================================================================

if (-not ([System.Management.Automation.PSTypeName]'DisplayNative').Type) {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class DisplayNative
{
    // ── Flags ─────────────────────────────────────────────────────────────
    public const uint QDC_ALL_PATHS          = 0x00000001;
    public const uint QDC_ONLY_ACTIVE_PATHS  = 0x00000002;
    public const uint QDC_VIRTUAL_MODE_AWARE = 0x00000010;

    public const uint SDC_TOPOLOGY_SUPPLIED        = 0x00000010;
    public const uint SDC_USE_SUPPLIED_DISPLAY_CONFIG = 0x00000020;
    public const uint SDC_APPLY                    = 0x00000080;
    public const uint SDC_SAVE_TO_DATABASE         = 0x00000200;
    public const uint SDC_ALLOW_CHANGES            = 0x00000400;
    public const uint SDC_ALLOW_PATH_ORDER_CHANGES = 0x00002000;
    public const uint SDC_VIRTUAL_MODE_AWARE_FLAG  = 0x00008000;

    public const uint DISPLAYCONFIG_PATH_ACTIVE    = 0x00000001;

    // DISPLAYCONFIG_DEVICE_INFO_TYPE values
    public const int  DI_GET_SOURCE_NAME      = 1;
    public const int  DI_GET_TARGET_NAME      = 2;
    public const int  DI_GET_ADVANCED_COLOR2  = 15;
    public const int  DI_SET_HDR_STATE        = 16;
    public const int  DI_GET_DPI_SCALE        = -3;
    public const int  DI_SET_DPI_SCALE        = -4;

    // ChangeDisplaySettingsEx flags
    public const uint CDS_UPDATEREGISTRY = 0x00000001;
    public const uint DM_PELSWIDTH       = 0x00080000;
    public const uint DM_PELSHEIGHT      = 0x00100000;
    public const uint DM_DISPLAYFREQ     = 0x00400000;
    public const uint ENUM_CURRENT_SETTINGS = 0xFFFFFFFF;
    public const int  DISP_CHANGE_SUCCESSFUL = 0;

    // DPI percentage table (index = relativeScale - minRelativeScale)
    public static readonly uint[] DpiValues = { 100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500 };

    // ── Structs ───────────────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    public struct LUID { public uint LowPart; public int HighPart; }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINTL { public int x; public int y; }

    [StructLayout(LayoutKind.Sequential)]
    public struct RECTL { public int left, top, right, bottom; }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_DEVICE_INFO_HEADER
    {
        public int  type;
        public uint size;
        public LUID adapterId;
        public uint id;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAYCONFIG_TARGET_DEVICE_NAME
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint   flags;
        public uint   outputTechnology;
        public ushort edidManufactureId;
        public ushort edidProductCodeId;
        public uint   connectorInstance;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
        public string monitorFriendlyDeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string monitorDevicePath;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAYCONFIG_SOURCE_DEVICE_NAME
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string viewGdiDeviceName;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_PATH_SOURCE_INFO
    {
        public LUID adapterId;
        public uint id;
        public uint modeInfoIdx;
        public uint statusFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_PATH_TARGET_INFO
    {
        public LUID adapterId;
        public uint id;
        public uint modeInfoIdx;
        public uint outputTechnology;
        public uint rotation;
        public uint scaling;
        public uint refreshRate_Numerator;
        public uint refreshRate_Denominator;
        public uint scanLineOrdering;
        public int  targetAvailable;
        public uint statusFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_PATH_INFO
    {
        public DISPLAYCONFIG_PATH_SOURCE_INFO sourceInfo;
        public DISPLAYCONFIG_PATH_TARGET_INFO targetInfo;
        public uint flags;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_2DREGION { public uint cx; public uint cy; }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_RATIONAL { public uint Numerator; public uint Denominator; }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_VIDEO_SIGNAL_INFO
    {
        public ulong                  pixelRate;
        public DISPLAYCONFIG_RATIONAL hSyncFreq;
        public DISPLAYCONFIG_RATIONAL vSyncFreq;
        public DISPLAYCONFIG_2DREGION activeSize;
        public DISPLAYCONFIG_2DREGION totalSize;
        public uint                   videoStandard;
        public uint                   scanLineOrdering;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_TARGET_MODE
    { public DISPLAYCONFIG_VIDEO_SIGNAL_INFO targetVideoSignalInfo; }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_SOURCE_MODE
    { public uint width; public uint height; public uint pixelFormat; public POINTL position; }

    [StructLayout(LayoutKind.Sequential)]
    public struct DISPLAYCONFIG_DESKTOP_IMAGE_INFO
    { public POINTL PathSourceSize; public RECTL DesktopImageRegion; public RECTL DesktopImageClip; }

    [StructLayout(LayoutKind.Explicit, Size = 64)]
    public struct DISPLAYCONFIG_MODE_INFO
    {
        [FieldOffset(0)]  public uint infoType;
        [FieldOffset(4)]  public uint id;
        [FieldOffset(8)]  public LUID adapterId;
        [FieldOffset(16)] public DISPLAYCONFIG_TARGET_MODE      targetMode;
        [FieldOffset(16)] public DISPLAYCONFIG_SOURCE_MODE      sourceMode;
        [FieldOffset(16)] public DISPLAYCONFIG_DESKTOP_IMAGE_INFO desktopImageInfo;
    }

    // DPI structs
    [StructLayout(LayoutKind.Sequential)]
    public struct DpiConfigGet
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public int minRelativeScale;
        public int currentRelativeScale;
        public int maxRelativeScale;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct DpiConfigSet
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public int relativeScale;
    }

    // HDR / advanced color structs
    [StructLayout(LayoutKind.Sequential)]
    public struct AdvancedColorInfo2
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint value;
        // Bit helpers
        public bool AdvancedColorSupported       { get { return (value & 0x01u) != 0; } }
        public bool AdvancedColorActive          { get { return (value & 0x02u) != 0; } }
        public bool AdvancedColorLimitedByPolicy { get { return (value & 0x08u) != 0; } }
        public bool HDRSupported                 { get { return (value & 0x10u) != 0; } }
        public bool HDREnabled                   { get { return (value & 0x20u) != 0; } }
        public bool WideColorSupported           { get { return (value & 0x40u) != 0; } }
        public bool WideColorEnabled             { get { return (value & 0x80u) != 0; } }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct SetHdrState
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint value; // 1 = enable, 0 = disable
    }

    // DEVMODEW for resolution enumeration / change
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODEW
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public ushort dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
        public uint   dmFields;
        public POINTL dmPosition;
        public uint   dmDisplayOrientation, dmDisplayFixedOutput;
        public short  dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public ushort dmLogPixels;
        public uint   dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
        public uint   dmICMMethod, dmICMIntent, dmMediaType, dmDitherType;
        public uint   dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight;
    }

    // ── P/Invoke ──────────────────────────────────────────────────────────

    [DllImport("user32.dll")]
    public static extern int GetDisplayConfigBufferSizes(uint flags, out uint numPaths, out uint numModes);

    [DllImport("user32.dll", EntryPoint = "QueryDisplayConfig")]
    public static extern int QueryDisplayConfig(uint flags, ref uint numPaths,
        [In, Out] DISPLAYCONFIG_PATH_INFO[] paths, ref uint numModes,
        [In, Out] DISPLAYCONFIG_MODE_INFO[] modes, IntPtr topology);

    [DllImport("user32.dll")]
    public static extern int SetDisplayConfig(uint numPaths, [In] DISPLAYCONFIG_PATH_INFO[] paths,
        uint numModes, [In] DISPLAYCONFIG_MODE_INFO[] modes, uint flags);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigGetDeviceInfo(ref DISPLAYCONFIG_TARGET_DEVICE_NAME req);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigGetDeviceInfo(ref DISPLAYCONFIG_SOURCE_DEVICE_NAME req);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigGetDeviceInfo(ref DpiConfigGet req);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigSetDeviceInfo(ref DpiConfigSet req);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigGetDeviceInfo(ref AdvancedColorInfo2 req);

    [DllImport("user32.dll")]
    public static extern int DisplayConfigSetDeviceInfo(ref SetHdrState req);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplaySettingsExW(string lpszDeviceName, uint iModeNum,
        ref DEVMODEW lpDevMode, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int ChangeDisplaySettingsExW(string lpszDeviceName, ref DEVMODEW lpDevMode,
        IntPtr hwnd, uint dwFlags, IntPtr lParam);

    // ── Static helper methods ─────────────────────────────────────────────

    public static void GetAllPaths(uint flags,
        out DISPLAYCONFIG_PATH_INFO[] paths, out DISPLAYCONFIG_MODE_INFO[] modes)
    {
        uint p, m;
        GetDisplayConfigBufferSizes(flags, out p, out m);
        paths = new DISPLAYCONFIG_PATH_INFO[p];
        modes = new DISPLAYCONFIG_MODE_INFO[m];
        QueryDisplayConfig(flags, ref p, paths, ref m, modes, IntPtr.Zero);
        Array.Resize(ref paths, (int)p);
        Array.Resize(ref modes, (int)m);
    }

    public static string GetGdiDeviceName(LUID adapterId, uint sourceId)
    {
        var req = new DISPLAYCONFIG_SOURCE_DEVICE_NAME();
        req.header.type = DI_GET_SOURCE_NAME;
        req.header.size = (uint)Marshal.SizeOf(req);
        req.header.adapterId = adapterId;
        req.header.id = sourceId;
        return DisplayConfigGetDeviceInfo(ref req) == 0 ? req.viewGdiDeviceName : null;
    }

    public static DISPLAYCONFIG_TARGET_DEVICE_NAME GetTargetDeviceName(LUID adapterId, uint targetId)
    {
        var req = new DISPLAYCONFIG_TARGET_DEVICE_NAME();
        req.header.type = DI_GET_TARGET_NAME;
        req.header.size = (uint)Marshal.SizeOf(req);
        req.header.adapterId = adapterId;
        req.header.id = targetId;
        DisplayConfigGetDeviceInfo(ref req);
        return req;
    }

    public static DpiConfigGet GetDpiInfo(LUID adapterId, uint sourceId)
    {
        var req = new DpiConfigGet();
        req.header.type = DI_GET_DPI_SCALE;
        req.header.size = (uint)Marshal.SizeOf(req);
        req.header.adapterId = adapterId;
        req.header.id = sourceId;
        DisplayConfigGetDeviceInfo(ref req);
        return req;
    }

    public static bool SetDpiPercent(LUID adapterId, uint sourceId, uint desiredPercent)
    {
        var info = GetDpiInfo(adapterId, sourceId);
        // Find index of desiredPercent in DpiValues
        int targetIdx = -1;
        for (int i = 0; i < DpiValues.Length; i++)
            if (DpiValues[i] == desiredPercent) { targetIdx = i; break; }
        if (targetIdx < 0) return false;
        // relativeScale = targetIdx + minRelativeScale
        int relativeScale = targetIdx + info.minRelativeScale;
        if (relativeScale < info.minRelativeScale || relativeScale > info.maxRelativeScale) return false;
        var set = new DpiConfigSet();
        set.header.type = DI_SET_DPI_SCALE;
        set.header.size = (uint)Marshal.SizeOf(set);
        set.header.adapterId = adapterId;
        set.header.id = sourceId;
        set.relativeScale = relativeScale;
        return DisplayConfigSetDeviceInfo(ref set) == 0;
    }

    public static uint GetDpiPercent(LUID adapterId, uint sourceId)
    {
        var info = GetDpiInfo(adapterId, sourceId);
        int idx = info.currentRelativeScale - info.minRelativeScale;
        if (idx < 0 || idx >= DpiValues.Length) return 100;
        return DpiValues[idx];
    }

    public static uint GetRecommendedDpiPercent(LUID adapterId, uint sourceId)
    {
        var info = GetDpiInfo(adapterId, sourceId);
        int idx = 0 - info.minRelativeScale;
        if (idx < 0 || idx >= DpiValues.Length) return 100;
        return DpiValues[idx];
    }

    public static AdvancedColorInfo2 GetAdvancedColorInfo(LUID adapterId, uint targetId)
    {
        var req = new AdvancedColorInfo2();
        req.header.type = DI_GET_ADVANCED_COLOR2;
        req.header.size = (uint)Marshal.SizeOf(req);
        req.header.adapterId = adapterId;
        req.header.id = targetId;
        DisplayConfigGetDeviceInfo(ref req);
        return req;
    }

    public static bool SetHdrEnabled(LUID adapterId, uint targetId, bool enabled)
    {
        var req = new SetHdrState();
        req.header.type = DI_SET_HDR_STATE;
        req.header.size = (uint)Marshal.SizeOf(req);
        req.header.adapterId = adapterId;
        req.header.id = targetId;
        req.value = (uint)(enabled ? 1 : 0);
        return DisplayConfigSetDeviceInfo(ref req) == 0;
    }

    public static List<DEVMODEW> GetDisplayModes(string gdiDeviceName)
    {
        var modes = new List<DEVMODEW>();
        var dm = new DEVMODEW();
        dm.dmSize = (ushort)Marshal.SizeOf(dm);
        uint i = 0;
        while (EnumDisplaySettingsExW(gdiDeviceName, i, ref dm, 0)) { modes.Add(dm); i++; }
        return modes;
    }

    public static DEVMODEW GetCurrentMode(string gdiDeviceName)
    {
        var dm = new DEVMODEW();
        dm.dmSize = (ushort)Marshal.SizeOf(dm);
        EnumDisplaySettingsExW(gdiDeviceName, ENUM_CURRENT_SETTINGS, ref dm, 0);
        return dm;
    }

    public static int SetDisplayMode(string gdiDeviceName, uint width, uint height, uint frequency)
    {
        var dm = new DEVMODEW();
        dm.dmSize             = (ushort)Marshal.SizeOf(dm);
        dm.dmPelsWidth        = width;
        dm.dmPelsHeight       = height;
        dm.dmDisplayFrequency = frequency;
        dm.dmFields           = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQ;
        return ChangeDisplaySettingsExW(gdiDeviceName, ref dm, IntPtr.Zero, CDS_UPDATEREGISTRY, IntPtr.Zero);
    }

    // Enable a monitor by pattern (adds to active set).
    // Pattern "@ADAPTER:HIGH-LOW:TARGETID" -> hardware identity match; anything else -> FriendlyName substring.
    public static int EnableMonitorByName(string monitorName)
    {
        DISPLAYCONFIG_PATH_INFO[] active; DISPLAYCONFIG_MODE_INFO[] activeModes;
        GetAllPaths(QDC_ONLY_ACTIVE_PATHS, out active, out activeModes);
        foreach (var p in active)
        {
            if (MatchPath(p, monitorName)) return 0; // already active
        }

        DISPLAYCONFIG_PATH_INFO[] all; DISPLAYCONFIG_MODE_INFO[] allModes;
        GetAllPaths(QDC_ALL_PATHS, out all, out allModes);

        DISPLAYCONFIG_PATH_INFO? found = null, fallback = null;
        foreach (var p in all)
        {
            if (MatchPath(p, monitorName))
            {
                if (p.targetInfo.targetAvailable != 0) { found = p; break; }
                if (fallback == null) fallback = p;
            }
        }
        if (found == null) found = fallback;
        if (found == null) return -1;

        var newPaths = new DISPLAYCONFIG_PATH_INFO[active.Length + 1];
        for (int i = 0; i < active.Length; i++) newPaths[i] = active[i];
        var tp = found.Value;
        tp.flags |= DISPLAYCONFIG_PATH_ACTIVE;
        tp.sourceInfo.modeInfoIdx = 0xFFFFFFFF;
        tp.targetInfo.modeInfoIdx = 0xFFFFFFFF;
        newPaths[active.Length] = tp;

        uint flags = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES;
        return SetDisplayConfig((uint)newPaths.Length, newPaths, (uint)activeModes.Length, activeModes, flags);
    }

    // Deactivate all monitors NOT matching any of keepNames.
    // Each keepName may be "@ADAPTER:HIGH-LOW:TARGETID" for hardware identity or a FriendlyName substring.
    public static int DeactivateAllExcept(string[] keepNames)
    {
        DISPLAYCONFIG_PATH_INFO[] paths; DISPLAYCONFIG_MODE_INFO[] modes;
        GetAllPaths(QDC_ONLY_ACTIVE_PATHS, out paths, out modes);
        for (int i = 0; i < paths.Length; i++)
        {
            bool keep = false;
            foreach (var kn in keepNames)
                if (MatchPath(paths[i], kn)) { keep = true; break; }
            if (!keep) paths[i].flags &= ~DISPLAYCONFIG_PATH_ACTIVE;
        }
        uint flags = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES;
        return SetDisplayConfig((uint)paths.Length, paths, (uint)modes.Length, modes, flags);
    }

    // Move named monitor to position (0,0) = primary.
    // Pattern "@ADAPTER:HIGH-LOW:TARGETID" -> hardware identity match; anything else -> FriendlyName substring.
    public static int SetPrimaryByName(string monitorName)
    {
        DISPLAYCONFIG_PATH_INFO[] paths; DISPLAYCONFIG_MODE_INFO[] modes;
        GetAllPaths(QDC_ONLY_ACTIVE_PATHS, out paths, out modes);
        int primaryIdx = -1;
        for (int i = 0; i < paths.Length; i++)
        {
            if (MatchPath(paths[i], monitorName)) { primaryIdx = i; break; }
        }
        if (primaryIdx < 0) return -1;
        uint modeIdx = paths[primaryIdx].sourceInfo.modeInfoIdx;
        if (modeIdx == 0xFFFFFFFF) return -2;
        int ox = modes[modeIdx].sourceMode.position.x;
        int oy = modes[modeIdx].sourceMode.position.y;
        if (ox == 0 && oy == 0) return 0;
        for (int i = 0; i < modes.Length; i++)
            if (modes[i].infoType == 1)
            {
                modes[i].sourceMode.position.x -= ox;
                modes[i].sourceMode.position.y -= oy;
            }
        uint flags = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES;
        return SetDisplayConfig((uint)paths.Length, paths, (uint)modes.Length, modes, flags);
    }

    private static string SourceKey(DISPLAYCONFIG_PATH_SOURCE_INFO s)
    {
        return s.adapterId.HighPart.ToString() + "_" + s.adapterId.LowPart.ToString() + "_" + s.id.ToString();
    }

    // Match a CCD path against a pattern string.
    // Pattern "@ADAPTER:HIGH-LOW:TARGETID" -> exact hardware identity match (adapterLUID + targetId).
    // Any other string -> FriendlyName substring match (case-insensitive), original behaviour.
    private static bool MatchPath(DISPLAYCONFIG_PATH_INFO p, string pattern)
    {
        if (pattern != null && pattern.StartsWith("@ADAPTER:"))
        {
            var rest  = pattern.Substring(9).Split(':');
            if (rest.Length == 2)
            {
                var lp = rest[0].Split('-');
                if (lp.Length == 2 &&
                    int.TryParse(lp[0],  out int  hi) &&
                    uint.TryParse(lp[1], out uint lo) &&
                    uint.TryParse(rest[1], out uint tid))
                {
                    return p.targetInfo.adapterId.HighPart == hi &&
                           p.targetInfo.adapterId.LowPart  == lo &&
                           p.targetInfo.id == tid;
                }
            }
            return false;
        }
        var n = GetTargetDeviceName(p.targetInfo.adapterId, p.targetInfo.id).monitorFriendlyDeviceName ?? "";
        return n.Length > 0 && n.IndexOf(pattern, StringComparison.OrdinalIgnoreCase) >= 0;
    }

    private static List<DISPLAYCONFIG_PATH_INFO> FindPathCandidates(DISPLAYCONFIG_PATH_INFO[] allPaths, string pattern)
    {
        var found = new List<DISPLAYCONFIG_PATH_INFO>();
        foreach (var p in allPaths)
        {
            if (MatchPath(p, pattern))
                found.Add(p);
        }
        found.Sort((a, b) => b.targetInfo.targetAvailable.CompareTo(a.targetInfo.targetAvailable));
        return found;
    }

    // Configure the full display topology in one SetDisplayConfig call.
    // extendPatterns: each display claims a unique GPU source (extended desktop).
    // cloneGroups: each inner string[] shares one source (mirror); first element is the source display.
    // Displays not mentioned are implicitly deactivated. Defaults to extend if cloneGroups is null/empty.
    public static int ConfigureTopology(string[] extendPatterns, string[][] cloneGroups)
    {
        DISPLAYCONFIG_PATH_INFO[] allPaths; DISPLAYCONFIG_MODE_INFO[] allModes;
        GetAllPaths(QDC_ALL_PATHS, out allPaths, out allModes);

        var result = new List<DISPLAYCONFIG_PATH_INFO>();
        var claimedSources = new HashSet<string>();

        // Process clone groups: all displays in each group share one source
        if (cloneGroups != null)
        {
            foreach (var group in cloneGroups)
            {
                if (group == null || group.Length == 0) continue;
                bool groupHasSrc = false;
                DISPLAYCONFIG_PATH_SOURCE_INFO sharedSrc = default(DISPLAYCONFIG_PATH_SOURCE_INFO);
                string sharedKey = null;

                foreach (var pattern in group)
                {
                    var cands = FindPathCandidates(allPaths, pattern);
                    if (cands.Count == 0) continue;
                    DISPLAYCONFIG_PATH_INFO tp = cands[0];

                    if (!groupHasSrc)
                    {
                        bool foundSrc = false;
                        foreach (var c in cands)
                        {
                            string k = SourceKey(c.sourceInfo);
                            if (claimedSources.Add(k))
                            {
                                tp = c; sharedSrc = c.sourceInfo; sharedKey = k; groupHasSrc = true; foundSrc = true; break;
                            }
                        }
                        if (!foundSrc) continue;
                    }
                    else
                    {
                        foreach (var c in cands) { if (SourceKey(c.sourceInfo) == sharedKey) { tp = c; break; } }
                    }

                    tp.flags |= DISPLAYCONFIG_PATH_ACTIVE;
                    tp.sourceInfo = sharedSrc;
                    tp.sourceInfo.modeInfoIdx = 0xFFFFFFFF;
                    tp.targetInfo.modeInfoIdx = 0xFFFFFFFF;
                    result.Add(tp);
                }
            }
        }

        // Process extend displays: each claims a unique source
        if (extendPatterns != null)
        {
            foreach (var pattern in extendPatterns)
            {
                var cands = FindPathCandidates(allPaths, pattern);
                bool added = false;
                foreach (var c in cands)
                {
                    string k = SourceKey(c.sourceInfo);
                    if (claimedSources.Add(k))
                    {
                        var tp = c;
                        tp.flags |= DISPLAYCONFIG_PATH_ACTIVE;
                        tp.sourceInfo.modeInfoIdx = 0xFFFFFFFF;
                        tp.targetInfo.modeInfoIdx = 0xFFFFFFFF;
                        result.Add(tp); added = true; break;
                    }
                }
                if (!added && cands.Count > 0)
                {
                    var tp = cands[0];
                    tp.flags |= DISPLAYCONFIG_PATH_ACTIVE;
                    tp.sourceInfo.modeInfoIdx = 0xFFFFFFFF;
                    tp.targetInfo.modeInfoIdx = 0xFFFFFFFF;
                    result.Add(tp);
                }
            }
        }

        if (result.Count == 0) return -1;
        uint flags2 = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE
                    | SDC_ALLOW_CHANGES | SDC_ALLOW_PATH_ORDER_CHANGES;
        return SetDisplayConfig((uint)result.Count, result.ToArray(), 0, new DISPLAYCONFIG_MODE_INFO[0], flags2);
    }
}
'@
}

#endregion SECTION: Win32_Types — DisplayNative

# =============================================================================
#region SECTION: Win32_Types — AudioNative (Core Audio COM, no module)
# =============================================================================

if (-not ([System.Management.Automation.PSTypeName]'AudioNative').Type) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;

// ── COM interfaces ────────────────────────────────────────────────────────────

[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceEnumerator
{
    int EnumAudioEndpoints(int dataFlow, uint stateMask, out IMMDeviceCollection ppDevices);
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice ppEndpoint);
    int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string pwstrId, out IMMDevice ppDevice);
    int RegisterEndpointNotificationCallback(IntPtr pClient);
    int UnregisterEndpointNotificationCallback(IntPtr pClient);
}

[ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceCollection
{
    int GetCount(out uint pcDevices);
    int Item(uint nDevice, out IMMDevice ppDevice);
}

[ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDevice
{
    int Activate(ref Guid iid, uint dwClsCtx, IntPtr pActivationParams, out IntPtr ppInterface);
    int OpenPropertyStore(uint stgmAccess, out IPropertyStore ppProperties);
    int GetId([MarshalAs(UnmanagedType.LPWStr)] out string ppstrId);
    int GetState(out uint pdwState);
}

[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPropertyStore
{
    int GetCount(out uint cProps);
    int GetAt(uint iProp, out PROPERTYKEY pkey);
    int GetValue(ref PROPERTYKEY key, out PROPVARIANT pv);
    int SetValue(ref PROPERTYKEY key, ref PROPVARIANT propvar);
    int Commit();
}

[StructLayout(LayoutKind.Sequential)]
public struct PROPERTYKEY { public Guid fmtid; public uint pid; }

[StructLayout(LayoutKind.Explicit, Size = 16)]
public struct PROPVARIANT
{
    [FieldOffset(0)] public ushort vt;
    [FieldOffset(8)] public IntPtr ptr;
}

[ComImport, Guid("5CDF2C82-841E-4546-9722-0CF74078229A"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IAudioEndpointVolume
{
    int RegisterControlChangeNotify(IntPtr pNotify);
    int UnregisterControlChangeNotify(IntPtr pNotify);
    int GetChannelCount(out uint pnChannelCount);
    int SetMasterVolumeLevel(float fLevelDB, ref Guid pguidEventContext);
    int SetMasterVolumeLevelScalar(float fLevel, ref Guid pguidEventContext);
    int GetMasterVolumeLevel(out float pfLevelDB);
    int GetMasterVolumeLevelScalar(out float pfLevel);
    int SetChannelVolumeLevel(uint nChannel, float fLevelDB, ref Guid pguidEventContext);
    int SetChannelVolumeLevelScalar(uint nChannel, float fLevel, ref Guid pguidEventContext);
    int GetChannelVolumeLevel(uint nChannel, out float pfLevelDB);
    int GetChannelVolumeLevelScalar(uint nChannel, out float pfLevel);
    int SetMute([MarshalAs(UnmanagedType.Bool)] bool bMute, ref Guid pguidEventContext);
    int GetMute([MarshalAs(UnmanagedType.Bool)] out bool pbMute);
    int GetVolumeStepInfo(out uint pnStep, out uint pnStepCount);
    int VolumeStepUp(ref Guid pguidEventContext);
    int VolumeStepDown(ref Guid pguidEventContext);
    int QueryHardwareSupport(out uint pdwHardwareSupportMask);
    int GetVolumeRange(out float pflMin, out float pflMax, out float pflIncrement);
}

// IPolicyConfig — undocumented but stable since Vista, works on Win10/11
[ComImport, Guid("F8679F50-850A-41CF-9C72-430F290290C8"),
 InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPolicyConfig
{
    [PreserveSig] int GetMixFormat([MarshalAs(UnmanagedType.LPWStr)] string dev, IntPtr ppFmt);
    [PreserveSig] int GetDeviceFormat([MarshalAs(UnmanagedType.LPWStr)] string dev,
        [MarshalAs(UnmanagedType.Bool)] bool bDefault, IntPtr ppFmt);
    [PreserveSig] int ResetDeviceFormat([MarshalAs(UnmanagedType.LPWStr)] string dev);
    [PreserveSig] int SetDeviceFormat([MarshalAs(UnmanagedType.LPWStr)] string dev,
        IntPtr pEndpoint, IntPtr pMix);
    [PreserveSig] int GetProcessingPeriod([MarshalAs(UnmanagedType.LPWStr)] string dev,
        [MarshalAs(UnmanagedType.Bool)] bool bDefault, IntPtr pmftDef, IntPtr pmftMin);
    [PreserveSig] int SetProcessingPeriod([MarshalAs(UnmanagedType.LPWStr)] string dev, IntPtr pmft);
    [PreserveSig] int GetShareMode([MarshalAs(UnmanagedType.LPWStr)] string dev, IntPtr pMode);
    [PreserveSig] int SetShareMode([MarshalAs(UnmanagedType.LPWStr)] string dev, IntPtr pMode);
    [PreserveSig] int GetPropertyValue([MarshalAs(UnmanagedType.LPWStr)] string dev,
        [MarshalAs(UnmanagedType.Bool)] bool bFxStore, ref PROPERTYKEY key, out PROPVARIANT pv);
    [PreserveSig] int SetPropertyValue([MarshalAs(UnmanagedType.LPWStr)] string dev,
        [MarshalAs(UnmanagedType.Bool)] bool bFxStore, ref PROPERTYKEY key, ref PROPVARIANT pv);
    [PreserveSig] int SetDefaultEndpoint([MarshalAs(UnmanagedType.LPWStr)] string devId, int role);
    [PreserveSig] int SetEndpointVisibility([MarshalAs(UnmanagedType.LPWStr)] string dev,
        [MarshalAs(UnmanagedType.Bool)] bool bVisible);
}

// ── AudioNative helper class ──────────────────────────────────────────────────

public class AudioDeviceInfo
{
    public string Id            { get; set; }
    public string FriendlyName  { get; set; }
    public string Type          { get; set; } // "Playback" or "Recording"
    public bool   IsDefault     { get; set; }
    public bool   IsDefaultComm { get; set; }
    public uint   State         { get; set; } // 1=Active
}

public static class AudioNative
{
    private static readonly Guid CLSID_MMDeviceEnumerator
        = new Guid("BCDE0395-E52F-467C-8E3D-C4579291692E");
    private static readonly Guid IID_IMMDeviceEnumerator
        = new Guid("A95664D2-9614-4F35-A746-DE8DB63617E6");
    private static readonly Guid IID_IAudioEndpointVolume
        = new Guid("5CDF2C82-841E-4546-9722-0CF74078229A");
    private static readonly Guid CLSID_PolicyConfigClient
        = new Guid("870AF99C-171D-4F9E-AF0D-E63DF40C2BC9");
    private static readonly Guid IID_IPolicyConfig
        = new Guid("F8679F50-850A-41CF-9C72-430F290290C8");

    // PKEY_Device_FriendlyName
    private static PROPERTYKEY FriendlyNameKey = new PROPERTYKEY
    {
        fmtid = new Guid("A45C254E-DF1C-4EFD-8020-67D146A850E0"),
        pid   = 14
    };

    public const uint DEVICE_STATE_ACTIVE = 0x00000001;
    public const uint DEVICE_STATEMASK_ALL = 0x0000000F;

    private static IMMDeviceEnumerator CreateEnumerator()
    {
        var type = Type.GetTypeFromCLSID(CLSID_MMDeviceEnumerator);
        return (IMMDeviceEnumerator)Activator.CreateInstance(type);
    }

    private static string GetFriendlyName(IMMDevice device)
    {
        IPropertyStore store;
        if (device.OpenPropertyStore(0 /* STGM_READ */, out store) != 0) return "";
        PROPVARIANT pv;
        PROPERTYKEY key = FriendlyNameKey;
        if (store.GetValue(ref key, out pv) != 0) return "";
        if (pv.vt != 31 /* VT_LPWSTR */ || pv.ptr == IntPtr.Zero) return "";
        return Marshal.PtrToStringUni(pv.ptr) ?? "";
    }

    public static AudioDeviceInfo[] GetAllDevices()
    {
        var result = new List<AudioDeviceInfo>();
        var enumerator = CreateEnumerator();

        // Get defaults for comparison
        IMMDevice defPlayback = null, defRecording = null,
                   defPlaybackComm = null, defRecordingComm = null;
        string defPbId = null, defRecId = null, defPbCommId = null, defRecCommId = null;
        try { enumerator.GetDefaultAudioEndpoint(0, 0, out defPlayback); defPlayback.GetId(out defPbId); } catch {}
        try { enumerator.GetDefaultAudioEndpoint(1, 0, out defRecording); defRecording.GetId(out defRecId); } catch {}
        try { enumerator.GetDefaultAudioEndpoint(0, 2, out defPlaybackComm); defPlaybackComm.GetId(out defPbCommId); } catch {}
        try { enumerator.GetDefaultAudioEndpoint(1, 2, out defRecordingComm); defRecordingComm.GetId(out defRecCommId); } catch {}

        foreach (int flow in new[] { 0, 1 }) // 0=Render/Playback, 1=Capture/Recording
        {
            IMMDeviceCollection col;
            if (enumerator.EnumAudioEndpoints(flow, DEVICE_STATEMASK_ALL, out col) != 0) continue;
            uint count;
            col.GetCount(out count);
            for (uint i = 0; i < count; i++)
            {
                IMMDevice dev;
                if (col.Item(i, out dev) != 0) continue;
                string id;
                dev.GetId(out id);
                uint state;
                dev.GetState(out state);
                string name = GetFriendlyName(dev);
                result.Add(new AudioDeviceInfo
                {
                    Id            = id,
                    FriendlyName  = name,
                    Type          = flow == 0 ? "Playback" : "Recording",
                    IsDefault     = (flow == 0 ? id == defPbId  : id == defRecId),
                    IsDefaultComm = (flow == 0 ? id == defPbCommId : id == defRecCommId),
                    State         = state
                });
            }
        }
        return result.ToArray();
    }

    public static int GetVolume(string deviceId)
    {
        var enumerator = CreateEnumerator();
        IMMDevice device;
        if (enumerator.GetDevice(deviceId, out device) != 0) return -1;
        Guid iid = IID_IAudioEndpointVolume;
        IntPtr ptr;
        if (device.Activate(ref iid, 0x17 /* CLSCTX_ALL */, IntPtr.Zero, out ptr) != 0) return -1;
        var vol = (IAudioEndpointVolume)Marshal.GetObjectForIUnknown(ptr);
        float level;
        vol.GetMasterVolumeLevelScalar(out level);
        Marshal.ReleaseComObject(vol);
        return (int)Math.Round(level * 100f);
    }

    public static bool SetVolume(string deviceId, int volumePercent)
    {
        var enumerator = CreateEnumerator();
        IMMDevice device;
        if (enumerator.GetDevice(deviceId, out device) != 0) return false;
        Guid iid = IID_IAudioEndpointVolume;
        IntPtr ptr;
        if (device.Activate(ref iid, 0x17, IntPtr.Zero, out ptr) != 0) return false;
        var vol = (IAudioEndpointVolume)Marshal.GetObjectForIUnknown(ptr);
        Guid empty = Guid.Empty;
        vol.SetMasterVolumeLevelScalar(volumePercent / 100f, ref empty);
        Marshal.ReleaseComObject(vol);
        return true;
    }

    public static bool GetMute(string deviceId)
    {
        var enumerator = CreateEnumerator();
        IMMDevice device;
        if (enumerator.GetDevice(deviceId, out device) != 0) return false;
        Guid iid = IID_IAudioEndpointVolume;
        IntPtr ptr;
        if (device.Activate(ref iid, 0x17, IntPtr.Zero, out ptr) != 0) return false;
        var vol = (IAudioEndpointVolume)Marshal.GetObjectForIUnknown(ptr);
        bool muted;
        vol.GetMute(out muted);
        Marshal.ReleaseComObject(vol);
        return muted;
    }

    public static bool SetMute(string deviceId, bool mute)
    {
        var enumerator = CreateEnumerator();
        IMMDevice device;
        if (enumerator.GetDevice(deviceId, out device) != 0) return false;
        Guid iid = IID_IAudioEndpointVolume;
        IntPtr ptr;
        if (device.Activate(ref iid, 0x17, IntPtr.Zero, out ptr) != 0) return false;
        var vol = (IAudioEndpointVolume)Marshal.GetObjectForIUnknown(ptr);
        Guid empty = Guid.Empty;
        vol.SetMute(mute, ref empty);
        Marshal.ReleaseComObject(vol);
        return true;
    }

    public static bool SetDefaultEndpoint(string deviceId)
    {
        try
        {
            var type = Type.GetTypeFromCLSID(CLSID_PolicyConfigClient);
            var policy = (IPolicyConfig)Activator.CreateInstance(type);
            // Set for all three roles: Console=0, Multimedia=1, Communications=2
            bool ok = policy.SetDefaultEndpoint(deviceId, 0) == 0;
            ok &= policy.SetDefaultEndpoint(deviceId, 1) == 0;
            ok &= policy.SetDefaultEndpoint(deviceId, 2) == 0;
            return ok;
        }
        catch { return false; }
    }
}
'@
}

#endregion SECTION: Win32_Types — AudioNative

# =============================================================================
#region SECTION: State_Store
# JSON devices.json — schema: { displays:{}, audio:{}, profiles:{} }
# =============================================================================

function Get-DefaultState {
    [PSCustomObject]@{ displays = [ordered]@{}; audio = [ordered]@{}; profiles = [ordered]@{} }
}

function Get-State {
    if (-not (Test-Path $Script:StateFile)) { Save-State (Get-DefaultState) }
    try {
        $raw = Get-Content -Path $Script:StateFile -Raw -ErrorAction Stop
        if ([string]::IsNullOrWhiteSpace($raw)) { return Get-DefaultState }
        return $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        Write-Log "State file corrupt: $($_.Exception.Message). Using empty state." -Level ERROR
        return Get-DefaultState
    }
}

function Save-State {
    param([Parameter(Mandatory)]$State)
    try { $State | ConvertTo-Json -Depth 12 | Set-Content -Path $Script:StateFile -Encoding UTF8 -ErrorAction Stop }
    catch { Write-Log "Failed to save state: $($_.Exception.Message)" -Level ERROR }
}

function ConvertTo-Hashtable {
    param($InputObject)
    if ($null -eq $InputObject) { return $null }
    if ($InputObject -is [System.Collections.IEnumerable] -and
        $InputObject -isnot [string] -and
        $InputObject -isnot [System.Collections.IDictionary]) {
        return @($InputObject | ForEach-Object { ConvertTo-Hashtable $_ })
    }
    if ($InputObject -is [PSCustomObject]) {
        $h = [ordered]@{}
        foreach ($p in $InputObject.PSObject.Properties) { $h[$p.Name] = ConvertTo-Hashtable $p.Value }
        return $h
    }
    return $InputObject
}

#endregion SECTION: State_Store

# =============================================================================
#region SECTION: Nickname_Registry
# =============================================================================

function Register-DisplayNickname {
    # Design: nickname maps to a physical monitor via hardware identity only.
    # No name-pattern matching — patterns are ambiguous for identical models.
    # Matching priority (see Get-DisplayNicknameFor):
    #   1. adapterLUID + targetId  (exact GPU port, runtime-stable within session)
    #   2. edidManufactureId + edidProductCodeId + connectorInstance [+ serial if stored]
    #      (stable across reboots when monitor stays on same port type)
    param(
        [Parameter(Mandatory)][string]$Nickname,
        [string]$Notes            = '',
        [string]$FriendlyName     = $null,
        [string]$AdapterLUID      = $null,
        [string]$TargetId         = $null,
        [int]   $ConnectorInstance = -1,
        [int]   $EdidManufactureId = 0,
        [int]   $EdidProductCodeId = 0,
        [string]$Serial           = $null
    )
    $state = ConvertTo-Hashtable (Get-State)
    if ($state.displays.Contains($Nickname)) {
        Write-Log "Overwriting display nickname '$Nickname'." -Level WARN
    }
    $state.displays[$Nickname] = [ordered]@{
        notes             = $Notes
        friendlyName      = $FriendlyName
        adapterLUID       = $AdapterLUID
        targetId          = $TargetId
        connectorInstance = $ConnectorInstance
        edidManufactureId = $EdidManufactureId
        edidProductCodeId = $EdidProductCodeId
        serial            = $Serial
    }
    Save-State $state
    Write-Log "Display nickname '$Nickname' -> '$FriendlyName' (conn#$ConnectorInstance) saved." -Level OK
}

function Register-AudioNickname {
    param(
        [Parameter(Mandatory)][string]$Nickname,
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][ValidateSet('Playback','Recording')][string]$Type,
        [string]$Notes = '',
        [string]$DeviceId = $null
    )
    $state = ConvertTo-Hashtable (Get-State)
    $state.audio[$Nickname] = [ordered]@{
        pattern  = $Pattern
        type     = $Type
        notes    = $Notes
        deviceId = $DeviceId
    }
    Save-State $state
    Write-Log "Audio nickname '$Nickname' -> '$Pattern' ($Type) saved." -Level OK
}

function Remove-Nickname {
    param(
        [Parameter(Mandatory)][ValidateSet('displays','audio')][string]$Kind,
        [Parameter(Mandatory)][string]$Nickname
    )
    $state = ConvertTo-Hashtable (Get-State)
    if ($state.$Kind.Contains($Nickname)) {
        $state.$Kind.Remove($Nickname); Save-State $state
        Write-Log "Removed $Kind nickname '$Nickname'." -Level OK
    } else { Write-Log "$Kind nickname '$Nickname' not found." -Level WARN }
}

function Get-DisplayNicknameFor {
    # Design: three-tier hardware-identity lookup, NO FriendlyName pattern matching.
    # Tier 1 — adapterLUID + targetId: exact GPU port (changes only on driver reinit).
    # Tier 2 — EDID mfr/product + connectorInstance [+ serial]: stable across reboots
    #           when the monitor stays on the same connector type.
    # Tier 3 — nothing found -> $null.
    param(
        [string]$FriendlyName     = '',   # informational only, not used for lookup
        [string]$Guid             = $null, # legacy param, unused
        [string]$AdapterLUID      = '',
        [string]$TargetId         = '',
        [int]   $ConnectorInstance = -1,
        [int]   $EdidManufactureId = 0,
        [int]   $EdidProductCodeId = 0,
        [string]$Serial           = ''
    )
    $state = Get-State
    if (-not $state.displays) { return $null }

    # Tier 1: exact GPU port
    if ($AdapterLUID -and $TargetId) {
        foreach ($nick in $state.displays.PSObject.Properties.Name) {
            $e = $state.displays.$nick
            if ($e.adapterLUID -and $e.targetId -and
                $e.adapterLUID -eq $AdapterLUID -and [string]$e.targetId -eq [string]$TargetId) {
                return $nick
            }
        }
    }

    # Tier 2: EDID identity + connector instance
    if ($EdidManufactureId -ne 0 -and $EdidProductCodeId -ne 0 -and $ConnectorInstance -ge 0) {
        foreach ($nick in $state.displays.PSObject.Properties.Name) {
            $e = $state.displays.$nick
            if ([int]$e.edidManufactureId -eq $EdidManufactureId -and
                [int]$e.edidProductCodeId -eq $EdidProductCodeId -and
                [int]$e.connectorInstance -eq $ConnectorInstance) {
                # If a serial is stored it must also match; absent serial on either side = accept
                if (-not $e.serial -or -not $Serial -or $e.serial -eq $Serial) {
                    return $nick
                }
            }
        }
    }

    return $null
}

function Get-AudioNicknameFor {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Type,
        [string]$DeviceId
    )
    $state = Get-State
    if (-not $state.audio) { return $null }
    foreach ($nick in $state.audio.PSObject.Properties.Name) {
        $e = $state.audio.$nick
        if ($e.type -eq $Type -and $e.pattern -and $Name -match [regex]::Escape($e.pattern)) { return $nick }
    }
    if ($DeviceId) {
        foreach ($nick in $state.audio.PSObject.Properties.Name) {
            $e = $state.audio.$nick
            if ($e.type -eq $Type -and $e.deviceId -and $e.deviceId -eq $DeviceId) { return $nick }
        }
    }
    return $null
}

function Test-DisplayNicknameDuplicates {
    # Design: checks hardware-identity keys for collisions; pattern/guid checks removed
    # because the new schema no longer stores those fields.
    $state = Get-State; if (-not $state.displays) { return }
    $seenPort = @{}   # "adapterLUID|targetId" -> nick
    $seenEdid = @{}   # "mfr|product|conn" -> nick
    foreach ($nick in $state.displays.PSObject.Properties.Name) {
        $e = $state.displays.$nick
        if ($e.adapterLUID -and $e.targetId) {
            $k = "$($e.adapterLUID)|$($e.targetId)"
            if ($seenPort.ContainsKey($k)) {
                Write-Log "Duplicate GPU port ($k): nicknames '$($seenPort[$k])' and '$nick'." -Level WARN
            } else { $seenPort[$k] = $nick }
        }
        if ($e.edidManufactureId -and $e.edidProductCodeId) {
            $k = "$($e.edidManufactureId)|$($e.edidProductCodeId)|$($e.connectorInstance)"
            if ($seenEdid.ContainsKey($k)) {
                Write-Log "Duplicate EDID identity ($k): nicknames '$($seenEdid[$k])' and '$nick'." -Level WARN
            } else { $seenEdid[$k] = $nick }
        }
    }
}

function Test-AudioNicknameDuplicates {
    $state = Get-State; if (-not $state.audio) { return }
    $seenPat = @{}; $seenId = @{}
    foreach ($nick in $state.audio.PSObject.Properties.Name) {
        $e = $state.audio.$nick
        $k = "$($e.type)|$($e.pattern)"
        if ($e.pattern) {
            if ($seenPat.ContainsKey($k)) {
                Write-Log "Duplicate audio pattern '$($e.pattern)' ($($e.type)): '$($seenPat[$k])' and '$nick'." -Level WARN
            } else { $seenPat[$k] = $nick }
        }
        if ($e.deviceId) {
            if ($seenId.ContainsKey($e.deviceId)) {
                Write-Log "Duplicate deviceId: '$($seenId[$e.deviceId])' and '$nick'." -Level WARN
            } else { $seenId[$e.deviceId] = $nick }
        }
    }
}

#endregion SECTION: Nickname_Registry

# =============================================================================
#region SECTION: Display_CCD_Low — raw path enumeration
# =============================================================================

function ConvertTo-OutputTechnologyName {
    param([uint32]$Value)
    switch ($Value) {
        0  {'VGA'}   1  {'S-Video'}  2  {'Composite'}  3  {'Component'}
        4  {'DVI'}   5  {'HDMI'}     6  {'LVDS'}        10 {'DisplayPort (ext)'}
        11 {'DisplayPort (emb)'}     12 {'UDI (ext)'}   13 {'UDI (emb)'}
        15 {'Miracast'}              16 {'Indirect (wired)'}
        17 {'Indirect (virtual)'}    2147483648 {'Internal'}
        default { "Unknown ($Value)" }
    }
}

function Get-DisplayDeviceGuid {
    param([string]$DevicePath)
    if ([string]::IsNullOrWhiteSpace($DevicePath)) { return $null }
    if ($DevicePath -match '\{([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\}\s*$') {
        return $Matches[1]
    }
    return $null
}

function Resolve-WmiSerial {
    param([string]$MonitorDevicePath, [hashtable]$WmiMap)
    # MonitorDevicePath: \\?\DISPLAY#HWID#INST&UID...#{GUID}
    # WMI InstanceName:  DISPLAY\HWID\INST_N
    if (-not $MonitorDevicePath -or $WmiMap.Count -eq 0) { return '' }
    $parts = ($MonitorDevicePath -replace '^\\\\\?\\', '') -split '#'
    if ($parts.Count -lt 3) { return '' }
    $hwId = $parts[1]
    $inst = ($parts[2] -split '&UID')[0]
    foreach ($key in $WmiMap.Keys) {
        $kp = $key -split '\\'
        if ($kp.Count -ge 3 -and $kp[1] -eq $hwId -and ($kp[2] -replace '_\d+$', '') -eq $inst) {
            return $WmiMap[$key]
        }
    }
    return ''
}

function Get-AllDisplayPaths {
    <#
    .SYNOPSIS
        Returns every display path (active+inactive) with resolved names,
        GDI device name, current mode, DPI, HDR, position, and serial number.
    #>
    [DisplayNative+DISPLAYCONFIG_PATH_INFO[]]$paths = $null
    [DisplayNative+DISPLAYCONFIG_MODE_INFO[]]$modes  = $null
    [DisplayNative]::GetAllPaths([DisplayNative]::QDC_ALL_PATHS, [ref]$paths, [ref]$modes)

    $wmiMap = @{}
    try {
        Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID -ErrorAction SilentlyContinue |
            ForEach-Object {
                $serial = ($_.SerialNumberID | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ }) -join ''
                if ($serial) { $wmiMap[$_.InstanceName] = $serial }
            }
    } catch {}

    $result = @()
    foreach ($path in $paths) {
        $devInfo  = [DisplayNative]::GetTargetDeviceName($path.targetInfo.adapterId, $path.targetInfo.id)
        $isActive = ($path.flags -band [DisplayNative]::DISPLAYCONFIG_PATH_ACTIVE) -ne 0

        $isPrimary   = $false
        $posX = $posY = 0
        $width = $height = $refreshHz = 0
        $gdiName = $null

        if ($isActive) {
            $srcIdx = $path.sourceInfo.modeInfoIdx
            if ($srcIdx -ne 0xFFFFFFFF) {
                $sm = $modes[$srcIdx].sourceMode
                $isPrimary = ($sm.position.x -eq 0 -and $sm.position.y -eq 0)
                $posX = $sm.position.x; $posY = $sm.position.y
                $width = $sm.width; $height = $sm.height
            }
            $tgtIdx = $path.targetInfo.modeInfoIdx
            if ($tgtIdx -ne 0xFFFFFFFF) {
                $vs = $modes[$tgtIdx].targetMode.targetVideoSignalInfo
                if ($vs.vSyncFreq.Denominator -gt 0) {
                    $refreshHz = [int][Math]::Round($vs.vSyncFreq.Numerator / $vs.vSyncFreq.Denominator)
                }
            }
            $gdiName = [DisplayNative]::GetGdiDeviceName($path.sourceInfo.adapterId, $path.sourceInfo.id)
        }

        # DPI (only for active paths with a valid source)
        $dpiPercent = $null
        $dpiRecommended = $null
        if ($isActive -and $path.sourceInfo.modeInfoIdx -ne 0xFFFFFFFF) {
            try {
                $dpiPercent     = [DisplayNative]::GetDpiPercent($path.sourceInfo.adapterId, $path.sourceInfo.id)
                $dpiRecommended = [DisplayNative]::GetRecommendedDpiPercent($path.sourceInfo.adapterId, $path.sourceInfo.id)
            } catch {}
        }

        # HDR
        $hdrSupported = $false; $hdrEnabled = $false
        try {
            $ci = [DisplayNative]::GetAdvancedColorInfo($path.targetInfo.adapterId, $path.targetInfo.id)
            $hdrSupported = $ci.HDRSupported
            $hdrEnabled   = $ci.HDREnabled
        } catch {}

        $result += [PSCustomObject]@{
            FriendlyName      = $devInfo.monitorFriendlyDeviceName
            Active            = $isActive
            Primary           = $isPrimary
            TargetAvailable   = [bool]$path.targetInfo.targetAvailable
            OutputTechnology  = ConvertTo-OutputTechnologyName -Value $path.targetInfo.outputTechnology
            ConnectorInstance = $devInfo.connectorInstance
            EdidManufactureId = [int]$devInfo.edidManufactureId
            EdidProductCodeId = [int]$devInfo.edidProductCodeId
            AdapterLUID       = "$($path.targetInfo.adapterId.HighPart)-$($path.targetInfo.adapterId.LowPart)"
            TargetId          = $path.targetInfo.id
            SourceAdapterLUID = "$($path.sourceInfo.adapterId.HighPart)-$($path.sourceInfo.adapterId.LowPart)"
            SourceId          = $path.sourceInfo.id
            GdiDeviceName     = $gdiName
            Width             = $width
            Height            = $height
            RefreshHz         = $refreshHz
            PositionX         = $posX
            PositionY         = $posY
            DpiPercent        = $dpiPercent
            DpiRecommended    = $dpiRecommended
            HdrSupported      = $hdrSupported
            HdrEnabled        = $hdrEnabled
            MonitorDevicePath = $devInfo.monitorDevicePath
            DeviceGuid        = Get-DisplayDeviceGuid -DevicePath $devInfo.monitorDevicePath
            SerialNumber      = Resolve-WmiSerial -MonitorDevicePath $devInfo.monitorDevicePath -WmiMap $wmiMap
            Rotation          = $path.targetInfo.rotation
        }
    }
    return $result
}

function Get-ActiveDisplayNames {
    [DisplayNative+DISPLAYCONFIG_PATH_INFO[]]$paths = $null
    [DisplayNative+DISPLAYCONFIG_MODE_INFO[]]$modes  = $null
    [DisplayNative]::GetAllPaths([DisplayNative]::QDC_ONLY_ACTIVE_PATHS, [ref]$paths, [ref]$modes)
    $names = @()
    foreach ($p in $paths) {
        $n = [DisplayNative]::GetTargetDeviceName($p.targetInfo.adapterId, $p.targetInfo.id).monitorFriendlyDeviceName
        if ($n) { $names += $n }
    }
    return $names
}

function Resolve-DisplayPathForNickname {
    <#
    .SYNOPSIS
        Returns the current active display path object for a nickname, resolved
        purely by hardware identity (adapterLUID+targetId first, EDID second).
        Returns $null if the physical monitor is not currently connected/active.
    #>
    # Design: single resolution point for all profile-apply and save-profile code.
    # Callers never need FriendlyName matching — hardware keys are unambiguous.
    param([Parameter(Mandatory)][string]$Nickname)
    $entry = (Get-State).displays.$Nickname
    if (-not $entry) { return $null }

    $allPaths = Get-AllDisplayPaths | Where-Object { $_.Active }

    # Tier 1: exact GPU port
    if ($entry.adapterLUID -and $entry.targetId) {
        $m = $allPaths | Where-Object {
            $_.AdapterLUID -eq $entry.adapterLUID -and [string]$_.TargetId -eq [string]$entry.targetId
        } | Select-Object -First 1
        if ($m) { return $m }
    }

    # Tier 2: EDID + connector instance
    if ([int]$entry.edidManufactureId -ne 0 -and [int]$entry.edidProductCodeId -ne 0) {
        $m = $allPaths | Where-Object {
            $_.EdidManufactureId -eq [int]$entry.edidManufactureId -and
            $_.EdidProductCodeId -eq [int]$entry.edidProductCodeId -and
            $_.ConnectorInstance -eq [int]$entry.connectorInstance -and
            (-not $entry.serial -or -not $_.SerialNumber -or $_.SerialNumber -eq $entry.serial)
        } | Select-Object -First 1
        if ($m) { return $m }
    }

    return $null
}

function Get-HardwareKeyForNickname {
    <#
    .SYNOPSIS
        Returns the "@ADAPTER:HIGH-LOW:TARGETID" hardware key string for a nickname.
        This key is understood by all DisplayNative C# methods (ConfigureTopology,
        EnableMonitorByName, SetPrimaryByName, etc.) and bypasses FriendlyName matching,
        which is ambiguous for identical monitor models.
        Returns $null if the nickname has no stored hardware identity.
    #>
    # Design: the @ADAPTER: key format routes by LUID+TargetId inside MatchPath()
    # — the single C# match predicate that accepts both hardware keys and legacy substrings.
    param([Parameter(Mandatory)][string]$Nickname)
    $entry = (Get-State).displays.$Nickname
    if (-not $entry -or -not $entry.adapterLUID -or -not $entry.targetId) { return $null }
    return "@ADAPTER:$($entry.adapterLUID):$($entry.targetId)"
}

#endregion SECTION: Display_CCD_Low

# =============================================================================
#region SECTION: Display_CCD_Mid — pattern-based enable/disable/primary
# =============================================================================

function Test-DisplayActive {
    # Design: @ADAPTER: keys route directly to hardware identity without FriendlyName lookup.
    # Legacy FriendlyName substring patterns still work for callers that haven't been updated.
    param([Parameter(Mandatory)][string]$Pattern)
    if ($Pattern.StartsWith('@ADAPTER:')) {
        $parts = $Pattern.Substring(9).Split(':')
        if ($parts.Count -eq 2) {
            $al = $parts[0]; $tid = $parts[1]
            return [bool](Get-AllDisplayPaths | Where-Object {
                $_.Active -and $_.AdapterLUID -eq $al -and [string]$_.TargetId -eq $tid
            })
        }
    }
    return [bool](Get-ActiveDisplayNames | Where-Object { $_ -match [regex]::Escape($Pattern) })
}

function Wait-DisplayState {
    param([Parameter(Mandatory)][string]$Pattern, [Parameter(Mandatory)][bool]$ShouldBeActive, [int]$TimeoutSec = 20)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 800
        if ((Test-DisplayActive $Pattern) -eq $ShouldBeActive) { return $true }
    }
    return $false
}

function Enable-DisplayByPattern {
    param([Parameter(Mandatory)][string]$Pattern, [int]$MaxAttempts = 5)
    if (Test-DisplayActive $Pattern) { Write-Log "'$Pattern' already active." -Level INFO; return $true }
    for ($i = 1; $i -le $MaxAttempts; $i++) {
        Write-Log "Enable '$Pattern' attempt $i/$MaxAttempts..." -Level INFO
        $ret = [DisplayNative]::EnableMonitorByName($Pattern)
        if ($ret -eq 0 -and (Wait-DisplayState -Pattern $Pattern -ShouldBeActive $true -TimeoutSec 15)) {
            Write-Log "Display '$Pattern' active." -Level OK; return $true
        }
        Start-Sleep -Milliseconds 2000
    }
    Write-Log "FAILED to activate '$Pattern' after $MaxAttempts attempts." -Level ERROR
    return $false
}

function Disable-DisplaysExcept {
    param([Parameter(Mandatory)][string[]]$KeepPatterns, [int]$MaxAttempts = 5)
    for ($i = 1; $i -le $MaxAttempts; $i++) {
        Write-Log "Disable all except [$(($KeepPatterns) -join ', ')] attempt $i..." -Level INFO
        $ret = [DisplayNative]::DeactivateAllExcept($KeepPatterns)
        if ($ret -ne 0) { Write-Log "DeactivateAllExcept ret $ret. Retrying..." -Level WARN; Start-Sleep -Milliseconds 2000; continue }
        Start-Sleep -Milliseconds 1500
        $still = Get-ActiveDisplayNames | Where-Object {
            $n = $_; -not ($KeepPatterns | Where-Object { $n -match [regex]::Escape($_) })
        }
        if (-not $still) { Write-Log "Deactivation confirmed." -Level OK; return $true }
        Start-Sleep -Milliseconds 2000
    }
    Write-Log "FAILED to deactivate non-kept displays." -Level ERROR; return $false
}

function Set-DisplayPrimaryByPattern {
    param([Parameter(Mandatory)][string]$Pattern, [int]$MaxAttempts = 5)
    for ($i = 1; $i -le $MaxAttempts; $i++) {
        if (-not (Test-DisplayActive $Pattern)) { Write-Log "'$Pattern' not active, cannot set primary." -Level ERROR; return $false }
        $ret = [DisplayNative]::SetPrimaryByName($Pattern)
        if ($ret -ge 0) {
            Start-Sleep -Milliseconds 1500
            $confirmed = if ($Pattern.StartsWith('@ADAPTER:')) {
                $kp = $Pattern.Substring(9).Split(':')
                [bool](Get-AllDisplayPaths | Where-Object { $_.AdapterLUID -eq $kp[0] -and [string]$_.TargetId -eq $kp[1] -and $_.Primary })
            } else {
                [bool](Get-AllDisplayPaths | Where-Object { $_.FriendlyName -match [regex]::Escape($Pattern) -and $_.Primary })
            }
            if ($confirmed) { Write-Log "'$Pattern' confirmed as primary." -Level OK; return $true }
            Write-Log "Not yet confirmed as primary. Retrying..." -Level WARN
        } else { Write-Log "SetPrimaryByName ret $ret. Retrying..." -Level WARN }
        Start-Sleep -Milliseconds 2000
    }
    Write-Log "FAILED to set '$Pattern' as primary." -Level ERROR; return $false
}

function Invoke-ExtendUntilActive {
    param([Parameter(Mandatory)][string[]]$RequiredPatterns, [int]$MaxAttempts = 5, [int]$TimeoutPerAttempt = 20)
    for ($i = 1; $i -le $MaxAttempts; $i++) {
        Write-Log "DisplaySwitch /extend attempt $i/$MaxAttempts..." -Level INFO
        & "$env:SystemRoot\System32\DisplaySwitch.exe" /extend
        Start-Sleep -Milliseconds 3000
        $allFound = $true
        foreach ($pat in $RequiredPatterns) {
            if (-not (Wait-DisplayState -Pattern $pat -ShouldBeActive $true -TimeoutSec $TimeoutPerAttempt)) {
                Write-Log "'$pat' not found after /extend." -Level WARN; $allFound = $false
            }
        }
        if ($allFound) { Write-Log "All displays active." -Level OK; return $true }
    }
    Write-Log "FAILED: required displays not all active." -Level ERROR; return $false
}

function Show-ActiveDisplays {
    $names = Get-ActiveDisplayNames
    if ($names.Count -eq 0) { Write-Host "    (none)" -ForegroundColor Gray; return }
    foreach ($n in $names) { Write-Host "    '$n'" -ForegroundColor Gray }
}

function Enable-AllKnownDisplays {
    # Design: use hardware key (@ADAPTER:) so identical-model monitors are routed unambiguously.
    $state = Get-State
    if (-not $state.displays) { Write-Log "No display nicknames registered." -Level WARN; return }
    foreach ($nick in $state.displays.PSObject.Properties.Name) {
        $hwKey = Get-HardwareKeyForNickname -Nickname $nick
        if ($hwKey) { Enable-DisplayByPattern -Pattern $hwKey | Out-Null }
        else { Write-Log "No hardware identity stored for '$nick', skipping." -Level WARN }
    }
}

#endregion SECTION: Display_CCD_Mid

# =============================================================================
#region SECTION: Display_Resolution — EnumDisplaySettings / ChangeDisplaySettings
# =============================================================================

$Script:ResolutionPresets = [ordered]@{
    '1080p  (1920x1080)'      = @{ Width = 1920; Height = 1080 }
    '1440p  (2560x1440)'      = @{ Width = 2560; Height = 1440 }
    '4K     (3840x2160)'      = @{ Width = 3840; Height = 2160 }
    '8K     (7680x4320)'      = @{ Width = 7680; Height = 4320 }
}
$Script:FrameRatePresets = @(30, 60, 90, 120, 144, 165, 200, 240)

function Get-DisplayModesForGdiDevice {
    <#
    .SYNOPSIS Low-level: enumerate all modes a GDI device supports. #>
    param([Parameter(Mandatory)][string]$GdiDeviceName)
    $rawModes = [DisplayNative]::GetDisplayModes($GdiDeviceName)
    $seen = @{}; $unique = @()
    foreach ($m in ($rawModes | Sort-Object dmPelsWidth, dmPelsHeight, dmDisplayFrequency)) {
        $key = "$($m.dmPelsWidth)x$($m.dmPelsHeight)@$($m.dmDisplayFrequency)"
        if (-not $seen.ContainsKey($key) -and $m.dmPelsWidth -gt 0) {
            $seen[$key] = $true
            $unique += [PSCustomObject]@{ Width = $m.dmPelsWidth; Height = $m.dmPelsHeight; Hz = $m.dmDisplayFrequency }
        }
    }
    return $unique
}

function Get-CurrentDisplayModeForGdiDevice {
    param([Parameter(Mandatory)][string]$GdiDeviceName)
    $dm = [DisplayNative]::GetCurrentMode($GdiDeviceName)
    return [PSCustomObject]@{ Width = $dm.dmPelsWidth; Height = $dm.dmPelsHeight; Hz = $dm.dmDisplayFrequency }
}

function Set-DisplayModeForGdiDevice {
    <#
    .SYNOPSIS Low-level: set resolution+refresh on a specific GDI device. #>
    param(
        [Parameter(Mandatory)][string]$GdiDeviceName,
        [Parameter(Mandatory)][uint32]$Width,
        [Parameter(Mandatory)][uint32]$Height,
        [Parameter(Mandatory)][uint32]$Hz
    )
    $ret = [DisplayNative]::SetDisplayMode($GdiDeviceName, $Width, $Height, $Hz)
    if ($ret -eq [DisplayNative]::DISP_CHANGE_SUCCESSFUL) {
        Write-Log "Mode ${Width}x${Height}@${Hz}Hz set on '$GdiDeviceName'." -Level OK; return $true
    }
    Write-Log "ChangeDisplaySettingsEx returned $ret for '$GdiDeviceName'." -Level ERROR; return $false
}

function Get-GdiDeviceNameForPattern {
    <#
    .SYNOPSIS Mid-level: find the GDI device name (\\.\DISPLAY1 etc.) for an active display matching Pattern. #>
    param([Parameter(Mandatory)][string]$Pattern)
    $all = Get-AllDisplayPaths
    $match = $all | Where-Object { $_.Active -and $_.FriendlyName -match [regex]::Escape($Pattern) } | Select-Object -First 1
    if (-not $match) { return $null }
    return $match.GdiDeviceName
}

function Get-DisplayModesForPattern {
    <#
    .SYNOPSIS Mid-level: enumerate modes for the active display matching Pattern. #>
    param([Parameter(Mandatory)][string]$Pattern)
    $gdi = Get-GdiDeviceNameForPattern -Pattern $Pattern
    if (-not $gdi) { Write-Log "Display '$Pattern' not active or not found." -Level WARN; return @() }
    return Get-DisplayModesForGdiDevice -GdiDeviceName $gdi
}

function Set-DisplayModeForPattern {
    <#
    .SYNOPSIS Mid-level: set resolution+refresh on the active display matching Pattern. #>
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][uint32]$Width,
        [Parameter(Mandatory)][uint32]$Height,
        [Parameter(Mandatory)][uint32]$Hz
    )
    $gdi = Get-GdiDeviceNameForPattern -Pattern $Pattern
    if (-not $gdi) { Write-Log "Display '$Pattern' not active - cannot set mode." -Level ERROR; return $false }
    return Set-DisplayModeForGdiDevice -GdiDeviceName $gdi -Width $Width -Height $Height -Hz $Hz
}

function Get-SupportedResolutionPresets {
    param([Parameter(Mandatory)][array]$Modes)
    $r = [ordered]@{}
    foreach ($label in $Script:ResolutionPresets.Keys) {
        $p = $Script:ResolutionPresets[$label]
        if ($Modes | Where-Object { $_.Width -eq $p.Width -and $_.Height -eq $p.Height }) { $r[$label] = $p }
    }
    return $r
}

function Get-SupportedFrameRates {
    param([Parameter(Mandatory)][array]$Modes, [Parameter(Mandatory)][uint32]$Width, [Parameter(Mandatory)][uint32]$Height)
    $available = $Modes | Where-Object { $_.Width -eq $Width -and $_.Height -eq $Height } | Select-Object -ExpandProperty Hz -Unique
    return $Script:FrameRatePresets | Where-Object { $available -contains $_ }
}

#endregion SECTION: Display_Resolution

# =============================================================================
#region SECTION: Display_DPI — per-monitor DPI via CCD
# =============================================================================

function Get-DisplayDpiInfo {
    <#
    .SYNOPSIS
        Returns current, recommended, min, max DPI % for a named display.
        Pattern is matched against the active display paths' friendly names.
    #>
    param([Parameter(Mandatory)][string]$Pattern)
    $all = Get-AllDisplayPaths
    $match = $all | Where-Object { $_.Active -and $_.FriendlyName -match [regex]::Escape($Pattern) } | Select-Object -First 1
    if (-not $match) { Write-Log "Display '$Pattern' not active." -Level WARN; return $null }

    # Re-query raw DPI info for min/max
    $adapterLuidParts = $match.SourceAdapterLUID -split '-'
    $luid = New-Object DisplayNative+LUID
    $luid.HighPart = [int]$adapterLuidParts[0]
    $luid.LowPart  = [uint32]$adapterLuidParts[1]
    $raw = [DisplayNative]::GetDpiInfo($luid, $match.SourceId)

    $minIdx = 0 - $raw.minRelativeScale
    $maxIdx = $raw.maxRelativeScale - $raw.minRelativeScale
    $minPct = if ($minIdx -ge 0 -and $minIdx -lt [DisplayNative]::DpiValues.Length) { [DisplayNative]::DpiValues[$minIdx] } else { 100 }
    $maxPct = if ($maxIdx -ge 0 -and $maxIdx -lt [DisplayNative]::DpiValues.Length) { [DisplayNative]::DpiValues[$maxIdx] } else { 500 }

    return [PSCustomObject]@{
        Pattern         = $Pattern
        FriendlyName    = $match.FriendlyName
        CurrentPercent  = $match.DpiPercent
        RecommendedPct  = $match.DpiRecommended
        MinPercent      = $minPct
        MaxPercent      = $maxPct
        SupportedValues = [DisplayNative]::DpiValues
    }
}

function Set-DisplayDpi {
    <#
    .SYNOPSIS Sets DPI % on the active display matching Pattern. Percent must be in DpiValues array. #>
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][ValidateSet(100,125,150,175,200,225,250,300,350,400,450,500)][uint32]$Percent
    )
    $all   = Get-AllDisplayPaths
    $match = $all | Where-Object { $_.Active -and $_.FriendlyName -match [regex]::Escape($Pattern) } | Select-Object -First 1
    if (-not $match) { Write-Log "Display '$Pattern' not active." -Level ERROR; return $false }

    $luidParts = $match.SourceAdapterLUID -split '-'
    $luid = New-Object DisplayNative+LUID
    $luid.HighPart = [int]$luidParts[0]
    $luid.LowPart  = [uint32]$luidParts[1]

    $ok = [DisplayNative]::SetDpiPercent($luid, $match.SourceId, $Percent)
    if ($ok) { Write-Log "DPI set to $Percent% on '$($match.FriendlyName)'." -Level OK }
    else     { Write-Log "Failed to set DPI $Percent% on '$($match.FriendlyName)'." -Level ERROR }
    return $ok
}

#endregion SECTION: Display_DPI

# =============================================================================
#region SECTION: Display_HDR — per-display HDR toggle
# =============================================================================

function Get-DisplayHdr {
    <#
    .SYNOPSIS Returns HDR info (Supported, Enabled) for the display matching Pattern. #>
    param([Parameter(Mandatory)][string]$Pattern)
    $all   = Get-AllDisplayPaths
    $match = $all | Where-Object { $_.FriendlyName -match [regex]::Escape($Pattern) } | Select-Object -First 1
    if (-not $match) { Write-Log "Display '$Pattern' not found." -Level WARN; return $null }
    return [PSCustomObject]@{
        FriendlyName = $match.FriendlyName
        Supported    = $match.HdrSupported
        Enabled      = $match.HdrEnabled
    }
}

function Set-DisplayHdr {
    <#
    .SYNOPSIS Enables or disables HDR on the display matching Pattern. #>
    param([Parameter(Mandatory)][string]$Pattern, [Parameter(Mandatory)][bool]$Enabled)
    $all   = Get-AllDisplayPaths
    $match = $all | Where-Object { $_.FriendlyName -match [regex]::Escape($Pattern) } | Select-Object -First 1
    if (-not $match) { Write-Log "Display '$Pattern' not found." -Level ERROR; return $false }

    $luidParts = $match.AdapterLUID -split '-'
    $luid = New-Object DisplayNative+LUID
    $luid.HighPart = [int]$luidParts[0]
    $luid.LowPart  = [uint32]$luidParts[1]

    $ok = [DisplayNative]::SetHdrEnabled($luid, $match.TargetId, $Enabled)
    if ($ok) { Write-Log "HDR $(if ($Enabled) {'enabled'} else {'disabled'}) on '$($match.FriendlyName)'." -Level OK }
    else     { Write-Log "Failed to $(if ($Enabled) {'enable'} else {'disable'}) HDR on '$($match.FriendlyName)'." -Level ERROR }
    return $ok
}

#endregion SECTION: Display_HDR

# =============================================================================
#region SECTION: Audio_Mid — pattern-based audio operations
# =============================================================================

function Get-AllAudioDevices {
    <#
    .SYNOPSIS Returns all audio devices as PSCustomObjects (wraps AudioNative.GetAllDevices). #>
    try { return [AudioNative]::GetAllDevices() }
    catch { Write-Log "Failed to enumerate audio devices: $($_.Exception.Message)" -Level ERROR; return @() }
}

function Show-AudioDevices {
    $devices = Get-AllAudioDevices
    if (-not $devices) { Write-Host "    (none)" -ForegroundColor Gray; return }
    foreach ($d in $devices) {
        $def  = if ($d.IsDefault)     { " [DEFAULT]"      } else { "" }
        $comm = if ($d.IsDefaultComm) { " [COMM DEFAULT]" } else { "" }
        $state = if ($d.State -eq 1)  { "Active"          } else { "Inactive($($d.State))" }
        Write-Host "  $($d.Type) | $($d.FriendlyName)$def$comm | $state" -ForegroundColor Gray
    }
}

function Resolve-AudioDeviceByPattern {
    <#
    .SYNOPSIS Finds exactly one audio device of Type whose name contains Pattern. #>
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][ValidateSet('Playback','Recording')][string]$Type
    )
    $devices = Get-AllAudioDevices
    $matches  = $devices | Where-Object { $_.Type -eq $Type -and $_.FriendlyName -match [regex]::Escape($Pattern) }
    if ($matches.Count -eq 1) { return $matches }
    if ($matches.Count -eq 0) { Write-Log "No $Type device matching '$Pattern'."                            -Level ERROR }
    else                      { Write-Log "Ambiguous: $($matches.Count) $Type devices match '$Pattern'."   -Level ERROR }
    return $null
}

function Set-AudioDeviceDefaultByPattern {
    param([Parameter(Mandatory)][string]$Pattern, [Parameter(Mandatory)][ValidateSet('Playback','Recording')][string]$Type)
    $dev = Resolve-AudioDeviceByPattern -Pattern $Pattern -Type $Type
    if (-not $dev) { return $false }
    $ok = [AudioNative]::SetDefaultEndpoint($dev.Id)
    if ($ok) { Write-Log "Default $Type set to '$($dev.FriendlyName)'." -Level OK }
    else     { Write-Log "Failed to set default $Type '$($dev.FriendlyName)'." -Level ERROR }
    return $ok
}

function Set-AudioDeviceVolumeByPattern {
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][ValidateSet('Playback','Recording')][string]$Type,
        [Parameter(Mandatory)][ValidateRange(0,100)][int]$Volume
    )
    $dev = Resolve-AudioDeviceByPattern -Pattern $Pattern -Type $Type
    if (-not $dev) { return $false }
    $ok = [AudioNative]::SetVolume($dev.Id, $Volume)
    if ($ok) { Write-Log "$Type volume '$($dev.FriendlyName)' -> $Volume%." -Level OK }
    else     { Write-Log "Failed to set volume on '$($dev.FriendlyName)'." -Level ERROR }
    return $ok
}

function Get-AudioDeviceVolumeByPattern {
    param([Parameter(Mandatory)][string]$Pattern, [Parameter(Mandatory)][ValidateSet('Playback','Recording')][string]$Type)
    $dev = Resolve-AudioDeviceByPattern -Pattern $Pattern -Type $Type
    if (-not $dev) { return -1 }
    return [AudioNative]::GetVolume($dev.Id)
}

function Set-AllAudioVolume {
    param([Parameter(Mandatory)][ValidateRange(0,100)][int]$Volume)
    $state = Get-State
    if (-not $state.audio) { Write-Log "No audio nicknames registered." -Level WARN; return }
    foreach ($nick in $state.audio.PSObject.Properties.Name) {
        $e = $state.audio.$nick
        Write-Log "Volume '$nick' ($($e.type)) -> $Volume%..." -Level STEP
        Set-AudioDeviceVolumeByPattern -Pattern $e.pattern -Type $e.type -Volume $Volume | Out-Null
    }
}

#endregion SECTION: Audio_Mid

# =============================================================================
#region SECTION: Profile_Management
# =============================================================================
#
# Profile schema (devices.json .profiles.<Name>):
# {
#   "displays": [
#     {
#       "nickname":   "<registered nickname>",
#       "active":     true|false,
#       "primary":    true|false,
#       "width":      <uint>,          # native resolution setting (new)
#       "height":     <uint>,
#       "refreshRate":<uint>,
#       "dpiPercent": <uint>|null,     # per-monitor DPI %
#       "hdr":        true|false|null, # HDR state
#       "rotation":   0|1|2|3|null     # 0=landscape,1=portrait,2=landscape(flip),3=portrait(flip)
#     }
#   ],
#   "audio": [
#     { "nickname": "<registered nickname>", "setDefault": true|false, "volume": 0-100|null }
#   ],
#   "startProcesses": [
#     { "path": "<exe>", "args": "<args>", "asAdmin": true|false }
#   ]
# }

function Get-Profiles {
    $state = Get-State
    if (-not $state.profiles) { return @() }
    return $state.profiles.PSObject.Properties.Name
}

function Show-Profiles {
    $names = Get-Profiles
    if (-not $names) { Write-Log "No profiles saved yet." -Level WARN; return }
    Write-Section "Known Profiles"
    foreach ($n in $names) { Write-Host "  - $n" }
}

function Remove-Profile {
    param([Parameter(Mandatory)][string]$Name)
    $state = ConvertTo-Hashtable (Get-State)
    if ($state.profiles.Contains($Name)) {
        $state.profiles.Remove($Name); Save-State $state
        Write-Log "Profile '$Name' removed." -Level OK
    } else { Write-Log "Profile '$Name' not found." -Level WARN }
}

function Save-Profile {
    <#
    .SYNOPSIS Saves/overwrites a profile from explicit spec objects. #>
    param(
        [Parameter(Mandatory)][string]$Name,
        [array]$Displays       = @(),
        [array]$Audio          = @(),
        [array]$StartProcesses = @()
    )
    $state = ConvertTo-Hashtable (Get-State)
    $state.profiles[$Name] = [ordered]@{
        displays       = $Displays
        audio          = $Audio
        startProcesses = $StartProcesses
    }
    Save-State $state
    Write-Log "Profile '$Name' saved." -Level OK
}

function Save-ProfileInteractive {
    <#
    .SYNOPSIS
        Captures the CURRENT live display+audio state and saves as a named profile.
        Only nicknamed devices are captured. Resolution/DPI/HDR are snapshotted from
        the current live state. -ResolutionPrompt controls whether to interactively
        override the resolution ('Console', 'Gui', 'None').
    #>
    # Design: path lookup uses Resolve-DisplayPathForNickname (hardware identity) instead of
    # FriendlyName pattern matching, so two identical monitors are distinguished correctly.
    param(
        [Parameter(Mandatory)][string]$Name,
        [ValidateSet('Console','Gui','None')][string]$ResolutionPrompt = 'Console'
    )
    $state = Get-State

    # Detect clone topology: displays sharing the same SourceAdapterLUID+SourceId are mirrored.
    # Use Resolve-DisplayPathForNickname so identical-model monitors are matched by hardware identity.
    $sourceToNicks = @{}
    if ($state.displays) {
        foreach ($nick in $state.displays.PSObject.Properties.Name) {
            $pi = Resolve-DisplayPathForNickname -Nickname $nick
            if ($pi) {
                $srcKey = "$($pi.SourceAdapterLUID)|$($pi.SourceId)"
                if (-not $sourceToNicks.ContainsKey($srcKey)) { $sourceToNicks[$srcKey] = @() }
                $sourceToNicks[$srcKey] += $nick
            }
        }
    }
    $mirrorOfMap = @{}  # nick -> mirrorOf nick (null = extend/source)
    foreach ($srcKey in $sourceToNicks.Keys) {
        $grp = $sourceToNicks[$srcKey]
        if ($grp.Count -lt 2) { continue }
        # Primary display in the group is the clone source; others mirror it
        $srcNick = $grp | Where-Object {
            $pi2 = Resolve-DisplayPathForNickname -Nickname $_
            $pi2 -and $pi2.Primary
        } | Select-Object -First 1
        if (-not $srcNick) { $srcNick = $grp[0] }
        foreach ($n in $grp) { if ($n -ne $srcNick) { $mirrorOfMap[$n] = $srcNick } }
    }

    $displaySpecs = @()
    if ($state.displays) {
        foreach ($nick in $state.displays.PSObject.Properties.Name) {
            $pathInfo = Resolve-DisplayPathForNickname -Nickname $nick
            $active   = $null -ne $pathInfo
            $primary  = $pathInfo -and $pathInfo.Primary

            # Snapshot current resolution/refresh
            $width = $height = $refreshRate = $null
            if ($active -and $pathInfo.Width -gt 0) {
                $width       = $pathInfo.Width
                $height      = $pathInfo.Height
                $refreshRate = $pathInfo.RefreshHz
            }

            # Allow override via interactive prompts
            if ($active -and $ResolutionPrompt -ne 'None') {
                $override = switch ($ResolutionPrompt) {
                    'Console' { Read-ResolutionForDisplay -Nickname $nick -CurrentWidth $width -CurrentHeight $height -CurrentHz $refreshRate -Pattern $pathInfo.FriendlyName }
                    'Gui'     { Show-ResolutionPickerDialog -Nickname $nick -Pattern $pathInfo.GdiDeviceName -CurrentWidth $width -CurrentHeight $height -CurrentHz $refreshRate }
                }
                if ($override) { $width = $override.Width; $height = $override.Height; $refreshRate = $override.Hz }
            }

            $displaySpecs += [ordered]@{
                nickname    = $nick
                active      = $active
                primary     = $primary
                width       = $width
                height      = $height
                refreshRate = $refreshRate
                dpiPercent  = if ($pathInfo) { $pathInfo.DpiPercent }  else { $null }
                hdr         = if ($pathInfo) { $pathInfo.HdrEnabled }  else { $null }
                rotation    = if ($pathInfo) { $pathInfo.Rotation }    else { $null }
                mirrorOf    = if ($mirrorOfMap.ContainsKey($nick)) { $mirrorOfMap[$nick] } else { $null }
            }
        }
    }

    $audioSpecs = @()
    if ($state.audio) {
        $liveDevices = Get-AllAudioDevices
        foreach ($nick in $state.audio.PSObject.Properties.Name) {
            $entry     = $state.audio.$nick
            $isDefault = [bool]($liveDevices | Where-Object {
                $_.Type -eq $entry.type -and $_.FriendlyName -match [regex]::Escape($entry.pattern) -and $_.IsDefault
            })
            $vol = -1
            $dev = $liveDevices | Where-Object { $_.Type -eq $entry.type -and $_.FriendlyName -match [regex]::Escape($entry.pattern) } | Select-Object -First 1
            if ($dev) { $vol = [AudioNative]::GetVolume($dev.Id) }
            $audioSpecs += [ordered]@{
                nickname   = $nick
                setDefault = $isDefault
                volume     = if ($vol -ge 0) { $vol } else { $null }
            }
        }
    }

    Save-Profile -Name $Name -Displays $displaySpecs -Audio $audioSpecs
    Write-Log "Profile '$Name' saved from live state." -Level OK
}

function Invoke-ProfilePostActions {
    param($PostActions)
    if (-not $PostActions) { return }
    foreach ($a in $PostActions) {
        Write-Log "Starting: $($a.path) $($a.args)" -Level STEP
        try {
            $p = @{ FilePath = $a.path; ErrorAction = 'Stop' }
            if ($a.args)    { $p.ArgumentList = $a.args }
            if ($a.asAdmin) { $p.Verb = 'RunAs' }
            Start-Process @p
        } catch { Write-Log "Failed to start '$($a.path)': $($_.Exception.Message)" -Level ERROR }
    }
}

function Invoke-Profile {
    <#
    .SYNOPSIS
        Applies a saved profile: activates/deactivates displays, sets primary,
        sets resolution, DPI, HDR per monitor, then audio defaults and volumes,
        then post-action processes.
    #>
    param([Parameter(Mandatory)][string]$Name)

    $state = Get-State
    if (-not $state.profiles -or -not ($state.profiles.PSObject.Properties.Name -contains $Name)) {
        Write-Log "Profile '$Name' not found." -Level ERROR; return
    }
    $prof = $state.profiles.$Name

    # Build nickname -> entry lookup tables
    $dispNicks  = @{}
    if ($state.displays) {
        foreach ($n in $state.displays.PSObject.Properties.Name) { $dispNicks[$n] = $state.displays.$n }
    }
    $audioNicks = @{}
    if ($state.audio) {
        foreach ($n in $state.audio.PSObject.Properties.Name)    { $audioNicks[$n] = $state.audio.$n }
    }

    Write-Section "Applying profile: $Name"

    # ── Steps 1-3: Configure topology (enable/disable/clone in one shot) ──
    # This fixes accidental mirror: QDC_ALL_PATHS paths can share source IDs, causing
    # sequential Enable calls to land two displays on the same GPU source (= clone mode).
    # ConfigureTopology explicitly de-conflicts sources for extend displays.
    Write-Log "[1/4] Configuring display topology..." -Level STEP

    # First pass: find which displays are mirrors of another (mirrorOf field).
    # Design: use hardware keys (@ADAPTER:) so identical-model monitors are unambiguous.
    $mirrorTargets = @{}  # sourceNick -> @(hwKey, ...)
    foreach ($d in $prof.displays) {
        if (-not $d.active -or -not $dispNicks.ContainsKey($d.nickname)) { continue }
        $mo = $d.mirrorOf
        if ($mo -and $dispNicks.ContainsKey($mo)) {
            if (-not $mirrorTargets.ContainsKey($mo)) { $mirrorTargets[$mo] = @() }
            $hwKey2 = Get-HardwareKeyForNickname -Nickname $d.nickname
            if ($hwKey2) { $mirrorTargets[$mo] += $hwKey2 }
        }
    }

    # Second pass: build extendPatterns and cloneGroups
    $extendPatterns = [System.Collections.Generic.List[string]]::new()
    $cloneGroupsList = [System.Collections.Generic.List[string[]]]::new()

    foreach ($d in $prof.displays) {
        if (-not $d.active -or -not $dispNicks.ContainsKey($d.nickname)) { continue }
        $mo = $d.mirrorOf
        if ($mo -and $dispNicks.ContainsKey($mo)) { continue }  # handled as part of clone group
        $hwKey = Get-HardwareKeyForNickname -Nickname $d.nickname
        if (-not $hwKey) { Write-Log "No hardware key for '$($d.nickname)', skipping." -Level WARN; continue }
        if ($mirrorTargets.ContainsKey($d.nickname)) {
            # This display is the source of a clone group
            $grp = @($hwKey) + $mirrorTargets[$d.nickname]
            [void]$cloneGroupsList.Add([string[]]$grp)
        } else {
            [void]$extendPatterns.Add($hwKey)
        }
    }

    $ret = [DisplayNative]::ConfigureTopology([string[]]$extendPatterns.ToArray(), [string[][]]$cloneGroupsList.ToArray())
    if ($ret -ne 0) { Write-Log "ConfigureTopology returned $ret." -Level WARN }
    Start-Sleep -Milliseconds 2000
    Show-ActiveDisplays

    # ── Step 2: Set primary ──
    Write-Log "[2/4] Setting primary display..." -Level STEP
    $primaryEntry = $prof.displays | Where-Object { $_.primary } | Select-Object -First 1
    if ($primaryEntry -and $dispNicks.ContainsKey($primaryEntry.nickname)) {
        $primaryKey = Get-HardwareKeyForNickname -Nickname $primaryEntry.nickname
        if ($primaryKey) {
            Set-DisplayPrimaryByPattern -Pattern $primaryKey | Out-Null
        } else { Write-Log "No hardware key for primary '$($primaryEntry.nickname)'." -Level WARN }
    } else { Write-Log "No primary display defined in profile." -Level WARN }

    # ── Step 3: Resolution, DPI, HDR per display ──
    # Design: resolve each nickname to its current live path object via hardware identity.
    # GdiDeviceName comes from the path, so no FriendlyName matching is needed for resolution.
    Write-Log "[3/4] Applying resolution / DPI / HDR..." -Level STEP
    foreach ($d in $prof.displays) {
        if (-not $d.active -or -not $dispNicks.ContainsKey($d.nickname)) { continue }
        $path = Resolve-DisplayPathForNickname -Nickname $d.nickname
        if (-not $path) { Write-Log "Nickname '$($d.nickname)' not connected, skipping settings." -Level WARN; continue }

        if ($d.width -and $d.height -and $d.refreshRate -and $path.GdiDeviceName) {
            Set-DisplayModeForGdiDevice -GdiDeviceName $path.GdiDeviceName -Width $d.width -Height $d.height -Hz $d.refreshRate | Out-Null
        }

        $effectiveDpi = if ($null -ne $d.dpiPercent -and $d.dpiPercent -gt 0) { $d.dpiPercent }
                        elseif ($null -ne $d.dpiScaling -and $d.dpiScaling -gt 0) { $d.dpiScaling }
                        else { $null }
        if ($null -ne $effectiveDpi) {
            $luidParts = $path.SourceAdapterLUID -split '-'
            $srcLuid = New-Object DisplayNative+LUID
            $srcLuid.HighPart = [int]$luidParts[0]; $srcLuid.LowPart = [uint32]$luidParts[1]
            $ok = [DisplayNative]::SetDpiPercent($srcLuid, $path.SourceId, $effectiveDpi)
            if ($ok) { Write-Log "DPI $effectiveDpi% set on '$($d.nickname)'." -Level OK }
            else     { Write-Log "Failed DPI on '$($d.nickname)'." -Level ERROR }
        }

        if ($null -ne $d.hdr) {
            $luidParts = $path.AdapterLUID -split '-'
            $tgtLuid = New-Object DisplayNative+LUID
            $tgtLuid.HighPart = [int]$luidParts[0]; $tgtLuid.LowPart = [uint32]$luidParts[1]
            $ok = [DisplayNative]::SetHdrEnabled($tgtLuid, [uint32]$path.TargetId, [bool]$d.hdr)
            if ($ok) { Write-Log "HDR $($d.hdr) set on '$($d.nickname)'." -Level OK }
            else     { Write-Log "Failed HDR on '$($d.nickname)'." -Level ERROR }
        }
    }

    # ── Step 5: Audio ──
    Write-Log "[4/4] Applying audio..." -Level STEP
    foreach ($a in $prof.audio) {
        if (-not $audioNicks.ContainsKey($a.nickname)) {
            Write-Log "Audio nickname '$($a.nickname)' not registered. Skipping." -Level WARN; continue
        }
        $entry = $audioNicks[$a.nickname]
        if ($a.setDefault) { Set-AudioDeviceDefaultByPattern -Pattern $entry.pattern -Type $entry.type | Out-Null }
        if ($null -ne $a.volume) { Set-AudioDeviceVolumeByPattern -Pattern $entry.pattern -Type $entry.type -Volume $a.volume | Out-Null }
    }

    Invoke-ProfilePostActions -PostActions $prof.startProcesses
    Write-Section "Profile '$Name' applied"
}

#endregion SECTION: Profile_Management

# =============================================================================
#region SECTION: Identification_Wizard — console
# =============================================================================

function Start-DeviceIdentificationWizard {
    Write-Section "Device Identification Wizard"

    Write-Host ""
    Write-Host "Checking for duplicate nicknames..." -ForegroundColor DarkGray
    Test-DisplayNicknameDuplicates
    Test-AudioNicknameDuplicates

    # ── Displays ──
    # Design: active-only so hardware identity (adapterLUID+targetId) is always valid and
    # EDID fields are populated. Inactive paths may have stale or zero EDID data.
    Write-Host ""
    Write-Host "-- Displays (active only) --" -ForegroundColor Yellow
    $rawDisplays = Get-AllDisplayPaths | Where-Object { $_.Active -and $_.FriendlyName }

    # Collapse duplicate CCD paths (same adapter+target+name) down to one entry
    $displayGroups = $rawDisplays | Group-Object -Property {
        "$($_.AdapterLUID)|$($_.TargetId)|$($_.FriendlyName)"
    }
    $displays = foreach ($g in $displayGroups) {
        $best = $g.Group | Sort-Object @{E={-not $_.Active}}, @{E={-not $_.TargetAvailable}} | Select-Object -First 1
        Add-Member -InputObject $best -NotePropertyName DuplicateCount -NotePropertyValue $g.Count -Force -PassThru
    }

    $i = 0; $displayList = @()
    foreach ($d in $displays) {
        $status  = if ($d.Primary) {"active, PRIMARY"} else {"active"}
        $nick    = Get-DisplayNicknameFor -FriendlyName $d.FriendlyName -Guid $d.DeviceGuid `
                       -AdapterLUID $d.AdapterLUID -TargetId ([string]$d.TargetId) `
                       -ConnectorInstance $d.ConnectorInstance `
                       -EdidManufactureId $d.EdidManufactureId -EdidProductCodeId $d.EdidProductCodeId `
                       -Serial $d.SerialNumber
        $nickTag = if ($nick) { "  [nickname: $nick]" } else { "" }
        $modeStr = if ($d.Width) { " $($d.Width)x$($d.Height)@$($d.RefreshHz)Hz" } else { "" }
        Write-Host ("  [{0}] {1}{2}  ({3})" -f $i, $d.FriendlyName, $modeStr, $status) -NoNewline
        if ($nick) { Write-Host $nickTag -ForegroundColor Green } else { Write-Host "" }
        Write-Host ("       via {0}, conn#{1}, adapter {2}" -f $d.OutputTechnology, $d.ConnectorInstance, $d.AdapterLUID) -ForegroundColor DarkGray
        Write-Host ("       EDID mfr=$($d.EdidManufactureId) prod=$($d.EdidProductCodeId)") -ForegroundColor DarkGray
        if ($d.SerialNumber)      { Write-Host "       serial: $($d.SerialNumber)" -ForegroundColor DarkGray }
        if ($d.MonitorDevicePath) { Write-Host "       path: $($d.MonitorDevicePath)" -ForegroundColor DarkGray }
        if ($d.DuplicateCount -gt 1) { Write-Host "       ($($d.DuplicateCount) duplicate paths collapsed)" -ForegroundColor DarkGray }
        $displayList += $d; $i++
    }

    # ── Audio ──
    Write-Host ""
    Write-Host "-- Audio devices --" -ForegroundColor Yellow
    $rawAudio     = Get-AllAudioDevices
    $audioGroups  = $rawAudio | Group-Object { "$($_.FriendlyName)|$($_.Type)" }
    $audioDeduped = @(foreach ($g in $audioGroups) {
        $best = $g.Group | Sort-Object @{E={-not $_.IsDefault}},@{E={$_.State -ne 1}} | Select-Object -First 1
        Add-Member -InputObject $best -NotePropertyName DuplicateCount -NotePropertyValue $g.Count -Force -PassThru
    })
    $j = 0; $audioList = @()
    foreach ($a in $audioDeduped) {
        $nick    = Get-AudioNicknameFor -Name $a.FriendlyName -Type $a.Type -DeviceId $a.Id
        $nickTag = if ($nick) { "  [nickname: $nick]" } else { "" }
        $def     = if ($a.IsDefault) { " [DEFAULT]" } else { "" }
        $dupNote = if ($a.DuplicateCount -gt 1) { " (+$($a.DuplicateCount - 1) dup)" } else { "" }
        Write-Host ("  [a{0}] {1} | {2}{3}{4}" -f $j, $a.Type, $a.FriendlyName, $def, $dupNote) -NoNewline
        if ($nick) { Write-Host $nickTag -ForegroundColor Green } else { Write-Host "" }
        $audioList += $a; $j++
    }

    Write-Host ""
    Write-Host "Enter display index, 'a<index>' for audio, or 'q' to quit." -ForegroundColor Cyan

    while ($true) {
        $choice = Read-Host "Selection"
        if ($choice -eq 'q' -or [string]::IsNullOrWhiteSpace($choice)) { break }

        if ($choice -match '^a(\d+)$') {
            $idx = [int]$Matches[1]
            if ($idx -lt 0 -or $idx -ge $audioList.Count) { Write-Host "Invalid audio index." -ForegroundColor Red; continue }
            $dev = $audioList[$idx]
            $existingNick = Get-AudioNicknameFor -Name $dev.FriendlyName -Type $dev.Type -DeviceId $dev.Id
            $suffix = if ($existingNick) { " [currently: $existingNick]" } else { "" }
            $nickname = Read-Host "  Nickname for '$($dev.FriendlyName)' ($($dev.Type))$suffix"
            if ([string]::IsNullOrWhiteSpace($nickname)) { continue }
            $pattern = Read-Host "  Match pattern [default: '$($dev.FriendlyName)']"
            if ([string]::IsNullOrWhiteSpace($pattern)) { $pattern = $dev.FriendlyName }
            if ($existingNick -and $existingNick -ne $nickname) { Remove-Nickname -Kind audio -Nickname $existingNick }
            Register-AudioNickname -Nickname $nickname -Pattern $pattern -Type $dev.Type -DeviceId $dev.Id
            Test-AudioNicknameDuplicates
        }
        elseif ($choice -match '^\d+$') {
            $idx = [int]$choice
            if ($idx -lt 0 -or $idx -ge $displayList.Count) { Write-Host "Invalid display index." -ForegroundColor Red; continue }
            $dev = $displayList[$idx]
            $existingNick = Get-DisplayNicknameFor -FriendlyName $dev.FriendlyName -Guid $dev.DeviceGuid `
                                -AdapterLUID $dev.AdapterLUID -TargetId ([string]$dev.TargetId) `
                                -ConnectorInstance $dev.ConnectorInstance `
                                -EdidManufactureId $dev.EdidManufactureId -EdidProductCodeId $dev.EdidProductCodeId `
                                -Serial $dev.SerialNumber
            $suffix = if ($existingNick) { " [currently: $existingNick]" } else { "" }
            $nickname = Read-Host "  Nickname for '$($dev.FriendlyName)'$suffix"
            if ([string]::IsNullOrWhiteSpace($nickname)) { continue }
            # No pattern prompt: matching is by hardware identity (adapterLUID+targetId / EDID)
            if ($existingNick -and $existingNick -ne $nickname) { Remove-Nickname -Kind displays -Nickname $existingNick }
            Register-DisplayNickname -Nickname $nickname -FriendlyName $dev.FriendlyName `
                -AdapterLUID $dev.AdapterLUID -TargetId ([string]$dev.TargetId) `
                -ConnectorInstance $dev.ConnectorInstance `
                -EdidManufactureId $dev.EdidManufactureId -EdidProductCodeId $dev.EdidProductCodeId `
                -Serial $dev.SerialNumber
            Test-DisplayNicknameDuplicates
        }
        else { Write-Host "Unrecognized input." -ForegroundColor Red }
    }
    Write-Log "Identification wizard done." -Level OK
}

#endregion SECTION: Identification_Wizard

# =============================================================================
#region SECTION: Console_Resolution_Picker
# =============================================================================

function Read-MenuChoice {
    param([Parameter(Mandatory)][string]$Title, [Parameter(Mandatory)][array]$Options, [switch]$AllowSkip)
    Write-Host $Title -ForegroundColor Cyan
    for ($k = 0; $k -lt $Options.Count; $k++) { Write-Host ("  [{0}] {1}" -f ($k+1), $Options[$k]) }
    if ($AllowSkip) { Write-Host "  [0] Skip" }
    while ($true) {
        $sel = Read-Host "  Selection"
        if ($AllowSkip -and ([string]::IsNullOrWhiteSpace($sel) -or $sel -eq '0')) { return -1 }
        if ($sel -match '^\d+$') { $n = [int]$sel; if ($n -ge 1 -and $n -le $Options.Count) { return ($n-1) } }
        Write-Host "  Invalid." -ForegroundColor Red
    }
}

function Read-ResolutionForDisplay {
    <#
    .SYNOPSIS Console picker: resolution + refresh rate for one display. Returns @{Width;Height;Hz} or $null. #>
    param(
        [Parameter(Mandatory)][string]$Nickname,
        [Parameter(Mandatory)][string]$Pattern,
        [uint32]$CurrentWidth, [uint32]$CurrentHeight, [uint32]$CurrentHz
    )
    Write-Host ""
    $cur = if ($CurrentWidth) { "${CurrentWidth}x${CurrentHeight}@${CurrentHz}Hz" } else { "unknown" }
    $setRes = Read-Host "Override resolution for '$Nickname' (current: $cur)? (y/N)"
    if ($setRes -ne 'y' -and $setRes -ne 'Y') { return $null }

    $modes = Get-DisplayModesForPattern -Pattern $Pattern
    if ($modes.Count -eq 0) {
        Write-Log "Could not query modes for '$Nickname' - enter manually." -Level WARN
        $w = [uint32](Read-Host "  Width (e.g. 1920)")
        $h = [uint32](Read-Host "  Height (e.g. 1080)")
        $hz= [uint32](Read-Host "  Refresh Hz (e.g. 60)")
        return @{ Width = $w; Height = $h; Hz = $hz }
    }

    $supported = Get-SupportedResolutionPresets -Modes $modes
    if ($supported.Count -eq 0) { $supported = $Script:ResolutionPresets }

    $resLabels = @($supported.Keys)
    $resIdx    = Read-MenuChoice -Title "  Resolution for '$Nickname':" -Options $resLabels -AllowSkip
    if ($resIdx -lt 0) { return $null }
    $chosenRes = $supported[$resLabels[$resIdx]]

    $rates = Get-SupportedFrameRates -Modes $modes -Width $chosenRes.Width -Height $chosenRes.Height
    if (-not $rates -or @($rates).Count -eq 0) { $rates = @($Script:FrameRatePresets) }
    $rateLabels = @($rates | ForEach-Object { "$_ Hz" })
    $rateIdx    = Read-MenuChoice -Title "  Frame rate for '$Nickname':" -Options $rateLabels -AllowSkip
    $chosenHz   = if ($rateIdx -lt 0) { $rates[0] } else { $rates[$rateIdx] }

    Write-Log "'$Nickname' -> $($chosenRes.Width)x$($chosenRes.Height)@${chosenHz}Hz" -Level OK
    return @{ Width = [uint32]$chosenRes.Width; Height = [uint32]$chosenRes.Height; Hz = [uint32]$chosenHz }
}

#endregion SECTION: Console_Resolution_Picker

# =============================================================================
#region SECTION: GUI_Resolution_Picker
# =============================================================================

function Show-ResolutionPickerDialog {
    <#
    .SYNOPSIS
        WinForms popup: pick resolution + refresh for ONE display.
        Returns @{Width;Height;Hz} or $null if skipped.
        If -ApplyNow is set, also calls Set-DisplayModeForPattern immediately.
    #>
    param(
        [Parameter(Mandatory)][string]$Nickname,
        [Parameter(Mandatory)][string]$Pattern,
        [uint32]$CurrentWidth, [uint32]$CurrentHeight, [uint32]$CurrentHz,
        [switch]$ApplyNow
    )
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing

    $script:RP_Modes   = @()
    $script:RP_Presets = $Script:ResolutionPresets
    $script:RP_Result  = $null

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Resolution - $Nickname"
    $form.Size = New-Object System.Drawing.Size(420, 340)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false; $form.MinimizeBox = $false; $form.TopMost = $true

    $cur = if ($CurrentWidth) { "${CurrentWidth}x${CurrentHeight} @ ${CurrentHz}Hz" } else { "unknown" }
    $lblTitle = New-Object System.Windows.Forms.Label
    $lblTitle.Text = "Display: $Nickname   (current: $cur)"
    $lblTitle.AutoSize = $true; $lblTitle.Top = 10; $lblTitle.Left = 10
    $lblTitle.Font = New-Object System.Drawing.Font($lblTitle.Font, [System.Drawing.FontStyle]::Bold)
    $form.Controls.Add($lblTitle)

    $lblStatus = New-Object System.Windows.Forms.Label
    $lblStatus.Text = "Loading modes..."
    $lblStatus.AutoSize = $false; $lblStatus.Top = 35; $lblStatus.Left = 10
    $lblStatus.Width = 380; $lblStatus.Height = 35
    $lblStatus.ForeColor = [System.Drawing.Color]::DarkGoldenrod
    $form.Controls.Add($lblStatus)

    $lblRes = New-Object System.Windows.Forms.Label
    $lblRes.Text = "Resolution:"; $lblRes.AutoSize = $true; $lblRes.Top = 80; $lblRes.Left = 10
    $form.Controls.Add($lblRes)

    $comboRes = New-Object System.Windows.Forms.ComboBox
    $comboRes.Top = 77; $comboRes.Left = 120; $comboRes.Width = 270; $comboRes.DropDownStyle = 'DropDownList'
    $form.Controls.Add($comboRes)

    $lblRate = New-Object System.Windows.Forms.Label
    $lblRate.Text = "Frame rate:"; $lblRate.AutoSize = $true; $lblRate.Top = 115; $lblRate.Left = 10
    $form.Controls.Add($lblRate)

    $comboRate = New-Object System.Windows.Forms.ComboBox
    $comboRate.Top = 112; $comboRate.Left = 120; $comboRate.Width = 270; $comboRate.DropDownStyle = 'DropDownList'
    $form.Controls.Add($comboRate)

    function SyncResCombo {
        $comboRes.Items.Clear()
        $script:RP_Presets = if ($script:RP_Modes.Count -gt 0) { Get-SupportedResolutionPresets -Modes $script:RP_Modes } else { $Script:ResolutionPresets }
        if ($script:RP_Presets.Count -eq 0) { $script:RP_Presets = $Script:ResolutionPresets }
        foreach ($label in $script:RP_Presets.Keys) { [void]$comboRes.Items.Add($label) }
        if ($comboRes.Items.Count -gt 0) { $comboRes.SelectedIndex = 0 }
        SyncRateCombo
    }

    function SyncRateCombo {
        $comboRate.Items.Clear()
        if (-not $comboRes.SelectedItem) { return }
        $chosenRes = $script:RP_Presets[$comboRes.SelectedItem]
        $rates = if ($script:RP_Modes.Count -gt 0) {
            Get-SupportedFrameRates -Modes $script:RP_Modes -Width $chosenRes.Width -Height $chosenRes.Height
        } else { $Script:FrameRatePresets }
        if (-not $rates -or @($rates).Count -eq 0) { $rates = @(60) }
        foreach ($hz in $rates) { [void]$comboRate.Items.Add("$hz Hz") }
        $comboRate.SelectedIndex = 0
    }

    $btnApply = New-Object System.Windows.Forms.Button
    $btnApply.Text = if ($ApplyNow) { "Apply Now" } else { "Use These Settings" }
    $btnApply.Top = 155; $btnApply.Left = 10; $btnApply.Width = 185; $btnApply.Height = 35
    $form.Controls.Add($btnApply)

    $btnSkip = New-Object System.Windows.Forms.Button
    $btnSkip.Text = "Skip"; $btnSkip.Top = 155; $btnSkip.Left = 205; $btnSkip.Width = 185; $btnSkip.Height = 35
    $form.Controls.Add($btnSkip)

    $comboRes.Add_SelectedIndexChanged({ SyncRateCombo })

    $btnApply.Add_Click({
        if (-not $comboRes.SelectedItem -or -not $comboRate.SelectedItem) {
            [System.Windows.Forms.MessageBox]::Show("Pick a resolution and frame rate first.") | Out-Null; return
        }
        $chosenRes = $script:RP_Presets[$comboRes.SelectedItem]
        $chosenHz  = [uint32]($comboRate.SelectedItem -replace '\s*Hz$', '')
        if ($ApplyNow) { Set-DisplayModeForPattern -Pattern $Pattern -Width $chosenRes.Width -Height $chosenRes.Height -Hz $chosenHz | Out-Null }
        $script:RP_Result = @{ Width = [uint32]$chosenRes.Width; Height = [uint32]$chosenRes.Height; Hz = $chosenHz }
        $form.DialogResult = [System.Windows.Forms.DialogResult]::OK; $form.Close()
    })
    $btnSkip.Add_Click({ $form.DialogResult = [System.Windows.Forms.DialogResult]::Cancel; $form.Close() })

    $form.Add_Shown({
        $form.Activate()
        # Load modes after form is visible so UI doesn't freeze
        $lblStatus.Text = "Querying supported modes..."
        [System.Windows.Forms.Application]::DoEvents()
        $script:RP_Modes = Get-DisplayModesForPattern -Pattern $Pattern
        $lblStatus.Text = if ($script:RP_Modes.Count -gt 0) {
            "Filtered to $($script:RP_Modes.Count) modes reported by Windows."
        } else {
            "Could not query modes - showing common presets."
        }
        SyncResCombo
    })

    [void]$form.ShowDialog()
    return $script:RP_Result
}

function Show-ResolutionPickerGuiForAllDisplays {
    <#
    .SYNOPSIS Pops one resolution picker per nicknamed display and applies each live. #>
    Add-Type -AssemblyName System.Windows.Forms
    $state = Get-State
    if (-not $state.displays -or @($state.displays.PSObject.Properties).Count -eq 0) {
        [System.Windows.Forms.MessageBox]::Show("No display nicknames registered. Run 'Identify Devices' first.") | Out-Null; return
    }
    foreach ($nick in $state.displays.PSObject.Properties.Name) {
        $path = Resolve-DisplayPathForNickname -Nickname $nick
        [void](Show-ResolutionPickerDialog -Nickname $nick -Pattern ($path.GdiDeviceName ?? $path.FriendlyName) `
            -CurrentWidth  ($path.Width)    `
            -CurrentHeight ($path.Height)   `
            -CurrentHz     ($path.RefreshHz)`
            -ApplyNow)
    }
}

#endregion SECTION: GUI_Resolution_Picker

# =============================================================================
#region SECTION: GUI_Identification_Wizard
# =============================================================================

function Show-MonitorIdentifyOverlays {
    <#
    .SYNOPSIS
        Flashes a numbered, full-screen overlay on every active monitor (like Windows Identify).
        Shows nickname (if assigned), friendly name, GDI device, resolution, and serial.
        Dismisses on click, any key, or after 5 seconds. Safe to call from within ShowDialog.
    #>
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing

    $activePaths = Get-AllDisplayPaths | Where-Object { $_.Active -and $_.GdiDeviceName }
    $screens     = [System.Windows.Forms.Screen]::AllScreens

    $overlayForms = [System.Collections.Generic.List[System.Windows.Forms.Form]]::new()
    $idx = 0
    foreach ($path in $activePaths) {
        $screen = $screens | Where-Object { $_.DeviceName -eq $path.GdiDeviceName } | Select-Object -First 1
        if (-not $screen) { continue }
        $idx++

        $nick  = if ($path.FriendlyName) {
            Get-DisplayNicknameFor -FriendlyName $path.FriendlyName -Guid $path.DeviceGuid `
                -AdapterLUID $path.AdapterLUID -TargetId ([string]$path.TargetId) -Serial $path.SerialNumber
        } else { $null }
        $label = if ($nick) { $nick } elseif ($path.FriendlyName) { $path.FriendlyName } else { $path.GdiDeviceName }
        $sub   = "$($path.GdiDeviceName)  |  $($path.Width)x$($path.Height)@$($path.RefreshHz)Hz"
        if ($path.SerialNumber) { $sub += "  |  S/N: $($path.SerialNumber)" }

        $form = New-Object System.Windows.Forms.Form
        $form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
        $form.TopMost         = $true
        $form.BackColor       = [System.Drawing.Color]::Black
        $form.Opacity         = 0.82
        # StartPosition must be Manual before Bounds is set; without it WinForms
        # ignores the explicit coordinates and places the form on the primary screen.
        $form.StartPosition   = [System.Windows.Forms.FormStartPosition]::Manual
        $form.Bounds          = $screen.Bounds
        $form.KeyPreview      = $true
        $form.ShowInTaskbar   = $false

        $lblNum  = New-Object System.Windows.Forms.Label
        $lblNum.Text      = "$idx"
        $lblNum.Font      = New-Object System.Drawing.Font('Segoe UI', 110, [System.Drawing.FontStyle]::Bold)
        $lblNum.ForeColor = [System.Drawing.Color]::White
        $lblNum.BackColor = [System.Drawing.Color]::Transparent
        $lblNum.AutoSize  = $true

        $lblName  = New-Object System.Windows.Forms.Label
        $lblName.Text      = $label
        $lblName.Font      = New-Object System.Drawing.Font('Segoe UI', 28, [System.Drawing.FontStyle]::Bold)
        $lblName.ForeColor = [System.Drawing.Color]::White
        $lblName.BackColor = [System.Drawing.Color]::Transparent
        $lblName.AutoSize  = $true

        $lblSub  = New-Object System.Windows.Forms.Label
        $lblSub.Text      = $sub
        $lblSub.Font      = New-Object System.Drawing.Font('Segoe UI', 13)
        $lblSub.ForeColor = [System.Drawing.Color]::FromArgb(200, 200, 200)
        $lblSub.BackColor = [System.Drawing.Color]::Transparent
        $lblSub.AutoSize  = $true

        $lblHint  = New-Object System.Windows.Forms.Label
        $lblHint.Text      = 'Click or press any key to dismiss'
        $lblHint.Font      = New-Object System.Drawing.Font('Segoe UI', 11)
        $lblHint.ForeColor = [System.Drawing.Color]::FromArgb(110, 110, 110)
        $lblHint.BackColor = [System.Drawing.Color]::Transparent
        $lblHint.AutoSize  = $true

        $form | Add-Member -NotePropertyName _n -NotePropertyValue $lblNum  -Force
        $form | Add-Member -NotePropertyName _t -NotePropertyValue $lblName -Force
        $form | Add-Member -NotePropertyName _s -NotePropertyValue $lblSub  -Force
        $form | Add-Member -NotePropertyName _h -NotePropertyValue $lblHint -Force
        $form.Controls.AddRange(@($lblNum, $lblName, $lblSub, $lblHint))

        $form.Add_Load({
            $w = $this.ClientSize.Width; $h = $this.ClientSize.Height
            $this._n.Left = [int](($w - $this._n.Width) / 2)
            $this._n.Top  = [int]($h / 2 - $this._n.Height - 14)
            $this._t.Left = [int](($w - $this._t.Width) / 2)
            $this._t.Top  = $this._n.Bottom + 14
            $this._s.Left = [int](($w - $this._s.Width) / 2)
            $this._s.Top  = $this._t.Bottom + 10
            $this._h.Left = [int](($w - $this._h.Width) / 2)
            $this._h.Top  = $h - 52
        })
        $form.Add_Click({ $this.Close() })
        $form.Add_KeyDown({ $this.Close() })

        $overlayForms.Add($form)
    }

    if ($overlayForms.Count -eq 0) { return }

    $script:_identForms = $overlayForms
    $timer = New-Object System.Windows.Forms.Timer
    $timer.Interval = 5000
    $script:_identTimer = $timer
    $timer.Add_Tick({
        $script:_identTimer.Stop()
        foreach ($f in $script:_identForms) { if (-not $f.IsDisposed) { $f.Close() } }
        [System.Windows.Forms.Application]::Exit()
    })

    foreach ($f in $overlayForms) { $f.Show() }
    $timer.Start()
    # Pump messages ourselves — avoids the "second message loop" error when called from ShowDialog
    while ($overlayForms | Where-Object { -not $_.IsDisposed -and $_.Visible }) {
        [System.Windows.Forms.Application]::DoEvents()
        [System.Threading.Thread]::Sleep(16)
    }
    $timer.Stop()
}

function Show-DeviceIdentificationGui {
    <#
    .SYNOPSIS WinForms device identification wizard — no console popup. #>
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing

    Test-DisplayNicknameDuplicates
    Test-AudioNicknameDuplicates

    # Enumerate + deduplicate displays — active only (inactive ports have no meaningful hardware identity)
    $rawDisplays   = Get-AllDisplayPaths | Where-Object { $_.Active -and $_.FriendlyName }
    $displayGroups = $rawDisplays | Group-Object { "$($_.AdapterLUID)|$($_.TargetId)|$($_.DeviceGuid)|$($_.FriendlyName)" }
    $dispList = @(foreach ($g in $displayGroups) {
        $best = $g.Group | Sort-Object @{E={-not $_.Active}},@{E={-not $_.TargetAvailable}} | Select-Object -First 1
        Add-Member -InputObject $best -NotePropertyName DuplicateCount -NotePropertyValue $g.Count -Force -PassThru
    })

    # Enumerate + deduplicate audio
    $rawAudio    = Get-AllAudioDevices
    $audioGroups = $rawAudio | Group-Object { "$($_.FriendlyName)|$($_.Type)" }
    $audioList   = @(foreach ($g in $audioGroups) {
        $best = $g.Group | Sort-Object @{E={-not $_.IsDefault}},@{E={$_.State -ne 1}} | Select-Object -First 1
        Add-Member -InputObject $best -NotePropertyName DuplicateCount -NotePropertyValue $g.Count -Force -PassThru
    })

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Device Identification Wizard"
    $form.ClientSize = New-Object System.Drawing.Size(680, 368)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false

    $tabs = New-Object System.Windows.Forms.TabControl
    $tabs.Top = 8; $tabs.Left = 8; $tabs.Width = 664; $tabs.Height = 280
    $form.Controls.Add($tabs)

    $tabD = New-Object System.Windows.Forms.TabPage
    $tabD.Text = "Displays ($($dispList.Count))"
    $tabs.TabPages.Add($tabD)

    $lvD = New-Object System.Windows.Forms.ListView
    $lvD.Dock = 'Fill'; $lvD.View = 'Details'
    $lvD.FullRowSelect = $true; $lvD.GridLines = $true; $lvD.MultiSelect = $false
    [void]$lvD.Columns.Add("Name",     185)
    [void]$lvD.Columns.Add("Status",    95)
    [void]$lvD.Columns.Add("Mode",     115)
    [void]$lvD.Columns.Add("Conn#",     50)
    [void]$lvD.Columns.Add("Nickname", 205)
    $tabD.Controls.Add($lvD)

    $tabA = New-Object System.Windows.Forms.TabPage
    $tabA.Text = "Audio"   # updated dynamically by $refreshAudio
    $tabs.TabPages.Add($tabA)

    # Filter bar at the bottom of the audio tab
    $pnlAudioFilter = New-Object System.Windows.Forms.Panel
    $pnlAudioFilter.Dock = 'Bottom'; $pnlAudioFilter.Height = 30
    $pnlAudioFilter.BorderStyle = 'FixedSingle'
    $tabA.Controls.Add($pnlAudioFilter)

    # "Show disabled" is first; "Also show duplicates" is second and independent.
    # They must be separate: a duplicate device can be disabled, so ANDing both filters
    # would require checking both boxes to see disabled duplicates — confusing and wrong.
    $chkShowDisabled = New-Object System.Windows.Forms.CheckBox
    $chkShowDisabled.Text = "Show disabled"; $chkShowDisabled.AutoSize = $true
    $chkShowDisabled.Top = 6; $chkShowDisabled.Left = 6; $chkShowDisabled.Checked = $false
    $pnlAudioFilter.Controls.Add($chkShowDisabled)

    $chkShowDups = New-Object System.Windows.Forms.CheckBox
    $chkShowDups.Text = "Also show duplicates"; $chkShowDups.AutoSize = $true
    $chkShowDups.Top = 6; $chkShowDups.Left = 180; $chkShowDups.Checked = $false
    $pnlAudioFilter.Controls.Add($chkShowDups)

    $lvA = New-Object System.Windows.Forms.ListView
    $lvA.Dock = 'Fill'; $lvA.View = 'Details'
    $lvA.FullRowSelect = $true; $lvA.GridLines = $true; $lvA.MultiSelect = $false
    [void]$lvA.Columns.Add("Name",     255)
    [void]$lvA.Columns.Add("Type",      80)
    [void]$lvA.Columns.Add("Default",   55)
    [void]$lvA.Columns.Add("Nickname", 195)
    $tabA.Controls.Add($lvA)

    $lblWizardNote = New-Object System.Windows.Forms.Label
    $lblWizardNote.Text = "Only nicknamed devices are included when saving profiles."
    $lblWizardNote.AutoSize = $true; $lblWizardNote.Top = 296; $lblWizardNote.Left = 8
    $lblWizardNote.ForeColor = [System.Drawing.Color]::DarkBlue
    $form.Controls.Add($lblWizardNote)

    $btnSetNick = New-Object System.Windows.Forms.Button
    $btnSetNick.Text = "Set Nickname..."; $btnSetNick.Top = 324; $btnSetNick.Left = 8
    $btnSetNick.Width = 190; $btnSetNick.Height = 34
    $form.Controls.Add($btnSetNick)

    $btnClearNick = New-Object System.Windows.Forms.Button
    $btnClearNick.Text = "Clear Nickname"; $btnClearNick.Top = 324; $btnClearNick.Left = 208
    $btnClearNick.Width = 150; $btnClearNick.Height = 34
    $form.Controls.Add($btnClearNick)

    $btnIdentify = New-Object System.Windows.Forms.Button
    $btnIdentify.Text = "Identify..."; $btnIdentify.Top = 324; $btnIdentify.Left = 368
    $btnIdentify.Width = 110; $btnIdentify.Height = 34
    $form.Controls.Add($btnIdentify)

    $btnDone = New-Object System.Windows.Forms.Button
    $btnDone.Text = "Done"; $btnDone.Top = 324; $btnDone.Left = 560
    $btnDone.Width = 112; $btnDone.Height = 34
    $form.Controls.Add($btnDone)

    # Script-block "methods" so closures in event handlers can invoke them via & operator
    # Pre-compute which FriendlyNames are shared by multiple monitors (identical models)
    $nameCount = @{}
    foreach ($d in $dispList) { $nameCount[$d.FriendlyName] = ($nameCount[$d.FriendlyName] ?? 0) + 1 }

    $refreshDisplays = {
        $lvD.Items.Clear()
        foreach ($d in $dispList) {
            $nick   = if ($d.FriendlyName) {
                Get-DisplayNicknameFor -FriendlyName $d.FriendlyName -Guid $d.DeviceGuid `
                    -AdapterLUID $d.AdapterLUID -TargetId ([string]$d.TargetId) `
                    -ConnectorInstance $d.ConnectorInstance `
                    -EdidManufactureId $d.EdidManufactureId -EdidProductCodeId $d.EdidProductCodeId `
                    -Serial $d.SerialNumber
            } else { $null }
            $status = if ($d.Active) { if ($d.Primary) { "active, PRIMARY" } else { "active" } } else { "inactive" }
            $mode   = if ($d.Active -and $d.Width) { "$($d.Width)x$($d.Height)@$($d.RefreshHz)Hz" } else { "" }
            # When multiple monitors share the same model name, append connector instance so they're distinguishable
            $displayName = if ($nameCount[$d.FriendlyName] -gt 1) {
                "$($d.FriendlyName) [conn#$($d.ConnectorInstance)]"
            } else { $d.FriendlyName }
            $item = New-Object System.Windows.Forms.ListViewItem($displayName)
            [void]$item.SubItems.Add($status)
            [void]$item.SubItems.Add($mode)
            [void]$item.SubItems.Add($d.ConnectorInstance)
            [void]$item.SubItems.Add($(if ($nick) { $nick } else { "" }))
            $item.Tag = $d
            if ($nick) { $item.ForeColor = [System.Drawing.Color]::DarkGreen }
            [void]$lvD.Items.Add($item)
        }
    }

    $refreshAudio = {
        $lvA.Items.Clear()

        # Count for checkbox labels (always from full list)
        $dupCount  = @($audioList | Where-Object { $_.DuplicateCount -gt 1 }).Count
        $disCount  = @($audioList | Where-Object { $_.State -ne 1 }).Count
        $chkShowDisabled.Text = "Show disabled ($disCount)"
        $chkShowDups.Text     = "Also show duplicates ($dupCount)"

        # Filters are independent. "Also show duplicates" overrides the disabled filter
        # for duplicate devices: if you asked to see duplicates you get all of them,
        # whether they are active or disabled. Otherwise the disabled gate applies alone.
        # This avoids the confusing situation where showDups has no visible effect unless
        # showDisabled is also checked (because the only duplicates on this system happen
        # to be disabled virtual endpoints).
        $filtered = $audioList | Where-Object {
            $isDup = $_.DuplicateCount -gt 1
            # Pass the duplicate gate: non-dups always pass; dups need showDups checked.
            ($chkShowDups.Checked -or -not $isDup) -and
            # Pass the disabled gate: active devices always pass; disabled need showDisabled
            # checked — UNLESS showDups is checked and this device is a dup (showDups wins).
            ($chkShowDisabled.Checked -or $_.State -eq 1 -or ($chkShowDups.Checked -and $isDup))
        }
        $tabA.Text = "Audio ($(@($filtered).Count))"

        foreach ($a in $filtered) {
            $nick = Get-AudioNicknameFor -Name $a.FriendlyName -Type $a.Type -DeviceId $a.Id
            $def  = if ($a.IsDefault) { "yes" } elseif ($a.IsDefaultComm) { "comm" } else { "" }
            $name = if ($a.DuplicateCount -gt 1) { "$($a.FriendlyName) (+$($a.DuplicateCount-1) dup)" } else { $a.FriendlyName }
            if ($a.State -eq 2) { $name = "[disabled] $name" }
            elseif ($a.State -eq 4) { $name = "[absent] $name" }
            elseif ($a.State -eq 8) { $name = "[unplugged] $name" }
            $item = New-Object System.Windows.Forms.ListViewItem($name)
            [void]$item.SubItems.Add($a.Type)
            [void]$item.SubItems.Add($def)
            [void]$item.SubItems.Add($(if ($nick) { $nick } else { "" }))
            $item.Tag = $a
            if    ($a.State -ne 1) { $item.ForeColor = [System.Drawing.Color]::Gray }
            elseif ($nick)          { $item.ForeColor = [System.Drawing.Color]::DarkGreen }
            [void]$lvA.Items.Add($item)
        }
        # Two non-interactive blank rows at the bottom so the action-button overlay
        # at the bottom of the panel does not obscure the last real list item.
        # Without these, scrolling to the bottom leaves the lowest device hidden behind
        # the "Set default" / "Set nickname" buttons.
        1..2 | ForEach-Object {
            $pad = New-Object System.Windows.Forms.ListViewItem("")
            $pad.ForeColor = [System.Drawing.Color]::Transparent
            $pad.Tag = $null
            [void]$lvA.Items.Add($pad)
        }
    }

    # Nickname input dialog — receives $ParentForm as param to stay window-constrained.
    # Design: Pattern field removed — matching is by hardware identity, not FriendlyName substring.
    $showNicknameDialog = {
        param(
            [string]$Title,
            [string]$CurrentNick,
            [System.Windows.Forms.Form]$ParentForm
        )
        $dlg = New-Object System.Windows.Forms.Form
        $dlg.Text = $Title
        $dlg.ClientSize = New-Object System.Drawing.Size(440, 110)
        $dlg.StartPosition = 'CenterParent'; $dlg.FormBorderStyle = 'FixedDialog'
        $dlg.MaximizeBox = $false; $dlg.MinimizeBox = $false

        $lbl2 = New-Object System.Windows.Forms.Label; $lbl2.Text = "Nickname:"
        $lbl2.AutoSize = $true; $lbl2.Top = 14; $lbl2.Left = 10; $dlg.Controls.Add($lbl2)
        $txtNick = New-Object System.Windows.Forms.TextBox
        $txtNick.Top = 11; $txtNick.Left = 90; $txtNick.Width = 330; $txtNick.Text = $CurrentNick
        $dlg.Controls.Add($txtNick)

        $lblNotes = New-Object System.Windows.Forms.Label; $lblNotes.Text = "Notes:"
        $lblNotes.AutoSize = $true; $lblNotes.Top = 44; $lblNotes.Left = 10; $dlg.Controls.Add($lblNotes)
        $txtNotes = New-Object System.Windows.Forms.TextBox
        $txtNotes.Top = 41; $txtNotes.Left = 90; $txtNotes.Width = 330
        $dlg.Controls.Add($txtNotes)

        $btnOk = New-Object System.Windows.Forms.Button; $btnOk.Text = "Save"
        $btnOk.Top = 70; $btnOk.Left = 120; $btnOk.Width = 90; $btnOk.Height = 30
        $btnOk.DialogResult = [System.Windows.Forms.DialogResult]::OK
        $dlg.AcceptButton = $btnOk; $dlg.Controls.Add($btnOk)

        $btnCan = New-Object System.Windows.Forms.Button; $btnCan.Text = "Cancel"
        $btnCan.Top = 70; $btnCan.Left = 225; $btnCan.Width = 90; $btnCan.Height = 30
        $btnCan.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
        $dlg.CancelButton = $btnCan; $dlg.Controls.Add($btnCan)

        $res = $dlg.ShowDialog($ParentForm)
        if ($res -eq [System.Windows.Forms.DialogResult]::OK -and -not [string]::IsNullOrWhiteSpace($txtNick.Text)) {
            return [ordered]@{ Nickname = $txtNick.Text.Trim(); Notes = $txtNotes.Text.Trim() }
        }
        return $null
    }

    $btnSetNick.Add_Click({
        if ($tabs.SelectedIndex -eq 0) {
            if (-not $lvD.SelectedItems.Count) {
                [System.Windows.Forms.MessageBox]::Show("Select a display first.", "No selection",
                    [System.Windows.Forms.MessageBoxButtons]::OK,
                    [System.Windows.Forms.MessageBoxIcon]::Information) | Out-Null; return
            }
            $d    = $lvD.SelectedItems[0].Tag
            $nick = Get-DisplayNicknameFor -FriendlyName $d.FriendlyName -Guid $d.DeviceGuid `
                        -AdapterLUID $d.AdapterLUID -TargetId ([string]$d.TargetId) `
                        -ConnectorInstance $d.ConnectorInstance `
                        -EdidManufactureId $d.EdidManufactureId -EdidProductCodeId $d.EdidProductCodeId `
                        -Serial $d.SerialNumber
            $res  = & $showNicknameDialog `
                -Title       "Nickname: $($d.FriendlyName)" `
                -CurrentNick $nick `
                -ParentForm  $form
            if ($res) {
                if ($nick -and $nick -ne $res.Nickname) { Remove-Nickname -Kind displays -Nickname $nick }
                Register-DisplayNickname -Nickname $res.Nickname -Notes $res.Notes `
                    -FriendlyName $d.FriendlyName -AdapterLUID $d.AdapterLUID -TargetId ([string]$d.TargetId) `
                    -ConnectorInstance $d.ConnectorInstance -EdidManufactureId $d.EdidManufactureId `
                    -EdidProductCodeId $d.EdidProductCodeId -Serial $d.SerialNumber
                Test-DisplayNicknameDuplicates
                & $refreshDisplays
            }
        } else {
            if (-not $lvA.SelectedItems.Count -or -not $lvA.SelectedItems[0].Tag) {
                [System.Windows.Forms.MessageBox]::Show("Select an audio device first.", "No selection",
                    [System.Windows.Forms.MessageBoxButtons]::OK,
                    [System.Windows.Forms.MessageBoxIcon]::Information) | Out-Null; return
            }
            $a    = $lvA.SelectedItems[0].Tag
            $nick = Get-AudioNicknameFor -Name $a.FriendlyName -Type $a.Type -DeviceId $a.Id
            $res  = & $showNicknameDialog `
                -Title       "Nickname: $($a.FriendlyName) ($($a.Type))" `
                -CurrentNick $nick `
                -ParentForm  $form
            if ($res) {
                if ($nick -and $nick -ne $res.Nickname) { Remove-Nickname -Kind audio -Nickname $nick }
                Register-AudioNickname -Nickname $res.Nickname -Pattern $a.FriendlyName -Type $a.Type -DeviceId $a.Id
                Test-AudioNicknameDuplicates
                & $refreshAudio
            }
        }
    })

    $btnClearNick.Add_Click({
        if ($tabs.SelectedIndex -eq 0) {
            if (-not $lvD.SelectedItems.Count) { return }
            $d    = $lvD.SelectedItems[0].Tag
            $nick = Get-DisplayNicknameFor -FriendlyName $d.FriendlyName -Guid $d.DeviceGuid `
                        -AdapterLUID $d.AdapterLUID -TargetId ([string]$d.TargetId) `
                        -ConnectorInstance $d.ConnectorInstance `
                        -EdidManufactureId $d.EdidManufactureId -EdidProductCodeId $d.EdidProductCodeId `
                        -Serial $d.SerialNumber
            if ($nick) { Remove-Nickname -Kind displays -Nickname $nick; & $refreshDisplays }
        } else {
            if (-not $lvA.SelectedItems.Count -or -not $lvA.SelectedItems[0].Tag) { return }
            $a    = $lvA.SelectedItems[0].Tag
            $nick = Get-AudioNicknameFor -Name $a.FriendlyName -Type $a.Type -DeviceId $a.Id
            if ($nick) { Remove-Nickname -Kind audio -Nickname $nick; & $refreshAudio }
        }
    })

    $btnIdentify.Add_Click({ Show-MonitorIdentifyOverlays })

    $lvD.Add_DoubleClick({ $btnSetNick.PerformClick() })
    $lvA.Add_DoubleClick({ $btnSetNick.PerformClick() })
    $chkShowDups.Add_CheckedChanged({ & $refreshAudio })
    $chkShowDisabled.Add_CheckedChanged({ & $refreshAudio })

    $btnDone.Add_Click({ $form.Close() })

    $form.Add_Shown({
        $form.Activate()
        & $refreshDisplays
        & $refreshAudio
    })
    [void]$form.ShowDialog()
}

#endregion SECTION: GUI_Identification_Wizard

# =============================================================================
#region SECTION: GUI_Profile_Switcher
# =============================================================================

function Show-ProfileSwitcherGui {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing, Microsoft.VisualBasic

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Display & Audio Orchestrator"
    $form.Size = New-Object System.Drawing.Size(520, 640)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false

    # ── Profile list ──────────────────────────────────────────────────────
    $lblProfiles = New-Object System.Windows.Forms.Label
    $lblProfiles.Text = "Profiles:"; $lblProfiles.AutoSize = $true; $lblProfiles.Top = 10; $lblProfiles.Left = 10
    $form.Controls.Add($lblProfiles)

    $listBox = New-Object System.Windows.Forms.ListBox
    $listBox.Top = 30; $listBox.Left = 10; $listBox.Width = 480; $listBox.Height = 130
    $form.Controls.Add($listBox)

    $lblHint = New-Object System.Windows.Forms.Label
    $lblHint.Text = "Double-click a profile to edit it."
    $lblHint.AutoSize = $true; $lblHint.Top = 162; $lblHint.Left = 12
    $lblHint.ForeColor = [System.Drawing.Color]::Gray
    $lblHint.Font = New-Object System.Drawing.Font($form.Font.FontFamily, 8)
    $form.Controls.Add($lblHint)

    function Refresh-ProfileList {
        $listBox.Items.Clear()
        foreach ($p in (Get-Profiles)) { $listBox.Items.Add($p) | Out-Null }
    }
    Refresh-ProfileList

    # ── Active displays status ────────────────────────────────────────────
    $lblActive = New-Object System.Windows.Forms.Label
    $lblActive.Text = "Active displays:"; $lblActive.AutoSize = $true; $lblActive.Top = 181; $lblActive.Left = 10
    $form.Controls.Add($lblActive)

    $txtStatus = New-Object System.Windows.Forms.TextBox
    $txtStatus.Top = 200; $txtStatus.Left = 10; $txtStatus.Width = 480; $txtStatus.Height = 45
    $txtStatus.Multiline = $true; $txtStatus.ReadOnly = $true
    $txtStatus.Font = New-Object System.Drawing.Font("Consolas", 8)
    $form.Controls.Add($txtStatus)

    function Refresh-Status {
        $names = Get-ActiveDisplayNames
        $txtStatus.Text = if ($names) { ($names | ForEach-Object { "  $_" }) -join "`r`n" } else { "  (none)" }
    }
    Refresh-Status

    # ── Buttons row 1 ────────────────────────────────────────────────────
    $btnApply     = New-Object System.Windows.Forms.Button
    $btnApply.Text = "Apply Profile"; $btnApply.Top = 253; $btnApply.Left = 10; $btnApply.Width = 150; $btnApply.Height = 35
    $form.Controls.Add($btnApply)

    $btnIdentify  = New-Object System.Windows.Forms.Button
    $btnIdentify.Text = "Identify Devices..."; $btnIdentify.Top = 253; $btnIdentify.Left = 170; $btnIdentify.Width = 150; $btnIdentify.Height = 35
    $form.Controls.Add($btnIdentify)

    $btnRefresh   = New-Object System.Windows.Forms.Button
    $btnRefresh.Text = "Refresh"; $btnRefresh.Top = 253; $btnRefresh.Left = 330; $btnRefresh.Width = 160; $btnRefresh.Height = 35
    $form.Controls.Add($btnRefresh)

    # ── Buttons row 2 ────────────────────────────────────────────────────
    $btnSave      = New-Object System.Windows.Forms.Button
    $btnSave.Text = "Save Current as Profile..."; $btnSave.Top = 296; $btnSave.Left = 10; $btnSave.Width = 230; $btnSave.Height = 35
    $btnSave.Enabled = $false
    $form.Controls.Add($btnSave)

    $btnDelete    = New-Object System.Windows.Forms.Button
    $btnDelete.Text = "Delete Profile"; $btnDelete.Top = 296; $btnDelete.Left = 250; $btnDelete.Width = 240; $btnDelete.Height = 35
    $form.Controls.Add($btnDelete)

    # ── Output log box ────────────────────────────────────────────────────
    $outputBox = New-Object System.Windows.Forms.TextBox
    $outputBox.Top = 340; $outputBox.Left = 10; $outputBox.Width = 480; $outputBox.Height = 247
    $outputBox.Multiline = $true; $outputBox.ScrollBars = 'Vertical'
    $outputBox.ReadOnly = $true; $outputBox.Font = New-Object System.Drawing.Font("Consolas", 8)
    $form.Controls.Add($outputBox)

    # ── Helper: sync button states to nickname existence ──────────────────
    $updateButtonStates = {
        $st = Get-State
        $hasNick = ($st.displays -and @($st.displays.PSObject.Properties).Count -gt 0) -or
                   ($st.audio    -and @($st.audio.PSObject.Properties).Count    -gt 0)
        $btnSave.Enabled = $hasNick
        if ($hasNick) {
            $btnIdentify.Font = New-Object System.Drawing.Font($btnIdentify.Font.FontFamily, $btnIdentify.Font.Size, [System.Drawing.FontStyle]::Regular)
        } else {
            $btnIdentify.Font = New-Object System.Drawing.Font($btnIdentify.Font.FontFamily, $btnIdentify.Font.Size, [System.Drawing.FontStyle]::Bold)
        }
    }

    # ── Event handlers ────────────────────────────────────────────────────
    $btnApply.Add_Click({
        if (-not $listBox.SelectedItem) { [System.Windows.Forms.MessageBox]::Show("Select a profile first.") | Out-Null; return }
        $outputBox.Clear()
        try {
            Invoke-Profile -Name $listBox.SelectedItem *>&1 | ForEach-Object {
                $outputBox.AppendText("$_`r`n")
                $outputBox.SelectionStart = $outputBox.Text.Length
                $outputBox.ScrollToCaret()
                [System.Windows.Forms.Application]::DoEvents()
            }
        } catch { $outputBox.AppendText("ERROR: $($_.Exception.Message)`r`n") }
        Refresh-Status
    })

    $btnIdentify.Add_Click({
        Show-DeviceIdentificationGui
        Refresh-ProfileList; Refresh-Status
        & $updateButtonStates
    })

    $btnRefresh.Add_Click({ Refresh-ProfileList; Refresh-Status })

    $listBox.Add_DoubleClick({
        if (-not $listBox.SelectedItem) { return }
        $profName = $listBox.SelectedItem
        $st2      = Get-State
        if (-not $st2.profiles -or -not $st2.profiles.$profName) {
            [System.Windows.Forms.MessageBox]::Show("Profile data not found.", "Error") | Out-Null; return
        }
        $prof = $st2.profiles.$profName

        $editForm = New-Object System.Windows.Forms.Form
        $editForm.Text = "Edit Profile: $profName"
        $editForm.Size = New-Object System.Drawing.Size(760, 600)
        $editForm.StartPosition = 'CenterParent'
        $editForm.FormBorderStyle = 'Sizable'
        $editForm.MaximizeBox = $true

        $outerTabs = New-Object System.Windows.Forms.TabControl
        $outerTabs.Top = 4; $outerTabs.Left = 4; $outerTabs.Width = 736; $outerTabs.Height = 514
        $outerTabs.Anchor = [System.Windows.Forms.AnchorStyles]::Top -bor
                            [System.Windows.Forms.AnchorStyles]::Bottom -bor
                            [System.Windows.Forms.AnchorStyles]::Left -bor
                            [System.Windows.Forms.AnchorStyles]::Right
        $editForm.Controls.Add($outerTabs)

        # ── Tab 1: Friendly editor ────────────────────────────────────────
        $tabFriendly = New-Object System.Windows.Forms.TabPage; $tabFriendly.Text = "Profile Editor"
        $outerTabs.TabPages.Add($tabFriendly)

        $resOptions = @(
            "(don't change)",
            "1920x1080 @ 60Hz",  "1920x1080 @ 120Hz",  "1920x1080 @ 144Hz",
            "2560x1440 @ 60Hz",  "2560x1440 @ 120Hz",  "2560x1440 @ 144Hz", "2560x1440 @ 165Hz",
            "3840x2160 @ 60Hz",  "3840x2160 @ 120Hz",  "3840x2160 @ 144Hz",
            "7680x4320 @ 60Hz"
        )
        $dpiOptions = @("(don't change)", "100%","125%","150%","175%","200%","225%","250%","300%","350%","400%","450%","500%")
        $hdrOptions = @("(don't change)", "On", "Off")

        # --- Displays DataGridView ---
        $lblDispSec = New-Object System.Windows.Forms.Label
        $lblDispSec.Text = "Displays"
        $lblDispSec.Font = New-Object System.Drawing.Font($editForm.Font.FontFamily, 9, [System.Drawing.FontStyle]::Bold)
        $lblDispSec.AutoSize = $true; $lblDispSec.Top = 6; $lblDispSec.Left = 6
        $tabFriendly.Controls.Add($lblDispSec)

        $dgvD = New-Object System.Windows.Forms.DataGridView
        $dgvD.Top = 26; $dgvD.Left = 4; $dgvD.Width = 720; $dgvD.Height = 140
        $dgvD.AllowUserToAddRows = $false; $dgvD.AllowUserToDeleteRows = $false
        $dgvD.RowHeadersVisible = $false; $dgvD.SelectionMode = 'FullRowSelect'
        $dgvD.AutoSizeColumnsMode = 'None'; $dgvD.EditMode = 'EditOnEnter'
        $dgvD.ScrollBars = 'Vertical'; $dgvD.ColumnHeadersHeightSizeMode = 'DisableResizing'
        $tabFriendly.Controls.Add($dgvD)
        $dgvD.Add_DataError({ $_.ThrowException = $false })

        $colDNick    = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
        $colDNick.HeaderText = "Nickname"; $colDNick.Width = 90; $colDNick.ReadOnly = $true; $colDNick.Name = "Nickname"
        $colDActive  = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
        $colDActive.HeaderText = "Active"; $colDActive.Width = 52; $colDActive.Name = "Active"
        $colDPrimary = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
        $colDPrimary.HeaderText = "Primary"; $colDPrimary.Width = 55; $colDPrimary.Name = "Primary"
        $colDRes     = New-Object System.Windows.Forms.DataGridViewComboBoxColumn
        $colDRes.HeaderText = "Resolution"; $colDRes.Width = 175; $colDRes.Name = "Resolution"; $colDRes.FlatStyle = 'Flat'
        foreach ($o in $resOptions) { [void]$colDRes.Items.Add($o) }
        $colDDpi     = New-Object System.Windows.Forms.DataGridViewComboBoxColumn
        $colDDpi.HeaderText = "DPI"; $colDDpi.Width = 100; $colDDpi.Name = "DPI"; $colDDpi.FlatStyle = 'Flat'
        foreach ($o in $dpiOptions) { [void]$colDDpi.Items.Add($o) }
        $colDHdr     = New-Object System.Windows.Forms.DataGridViewComboBoxColumn
        $colDHdr.HeaderText = "HDR"; $colDHdr.Width = 90; $colDHdr.Name = "HDR"; $colDHdr.FlatStyle = 'Flat'
        foreach ($o in $hdrOptions) { [void]$colDHdr.Items.Add($o) }
        $colDMirror  = New-Object System.Windows.Forms.DataGridViewComboBoxColumn
        $colDMirror.HeaderText = "Mirror Of"; $colDMirror.Width = 100; $colDMirror.Name = "MirrorOf"; $colDMirror.FlatStyle = 'Flat'
        [void]$colDMirror.Items.Add("(extend)")
        foreach ($d2 in $prof.displays) { [void]$colDMirror.Items.Add($d2.nickname) }
        [void]$dgvD.Columns.Add($colDNick); [void]$dgvD.Columns.Add($colDActive)
        [void]$dgvD.Columns.Add($colDPrimary); [void]$dgvD.Columns.Add($colDRes)
        [void]$dgvD.Columns.Add($colDDpi); [void]$dgvD.Columns.Add($colDHdr)
        [void]$dgvD.Columns.Add($colDMirror)

        foreach ($d in $prof.displays) {
            $resStr = if ($d.width -and $d.height -and $d.refreshRate) { "$($d.width)x$($d.height) @ $($d.refreshRate)Hz" } else { "(don't change)" }
            if ($resStr -ne "(don't change)" -and $colDRes.Items.IndexOf($resStr) -lt 0) { [void]$colDRes.Items.Add($resStr) }
            $dpiEffective = if ($null -ne $d.dpiPercent -and $d.dpiPercent -gt 0) { $d.dpiPercent }
                            elseif ($null -ne $d.dpiScaling -and $d.dpiScaling -gt 0) { $d.dpiScaling }
                            else { $null }
            $dpiStr = if ($null -ne $dpiEffective) { "$dpiEffective%" } else { "(don't change)" }
            if ($dpiStr -ne "(don't change)" -and $colDDpi.Items.IndexOf($dpiStr) -lt 0) { [void]$colDDpi.Items.Add($dpiStr) }
            $hdrStr    = if ($null -eq $d.hdr) { "(don't change)" } elseif ([bool]$d.hdr) { "On" } else { "Off" }
            $mirrorStr = if ($d.mirrorOf -and $colDMirror.Items.IndexOf($d.mirrorOf) -ge 0) { $d.mirrorOf } else { "(extend)" }
            [void]$dgvD.Rows.Add($d.nickname, [bool]$d.active, [bool]$d.primary, $resStr, $dpiStr, $hdrStr, $mirrorStr)
        }

        # --- Audio DataGridView ---
        $lblAudioSec = New-Object System.Windows.Forms.Label
        $lblAudioSec.Text = "Audio"
        $lblAudioSec.Font = New-Object System.Drawing.Font($editForm.Font.FontFamily, 9, [System.Drawing.FontStyle]::Bold)
        $lblAudioSec.AutoSize = $true; $lblAudioSec.Top = 174; $lblAudioSec.Left = 6
        $tabFriendly.Controls.Add($lblAudioSec)

        $dgvA = New-Object System.Windows.Forms.DataGridView
        $dgvA.Top = 194; $dgvA.Left = 4; $dgvA.Width = 720; $dgvA.Height = 110
        $dgvA.AllowUserToAddRows = $false; $dgvA.AllowUserToDeleteRows = $false
        $dgvA.RowHeadersVisible = $false; $dgvA.SelectionMode = 'FullRowSelect'
        $dgvA.AutoSizeColumnsMode = 'None'; $dgvA.EditMode = 'EditOnEnter'
        $dgvA.ScrollBars = 'Vertical'; $dgvA.ColumnHeadersHeightSizeMode = 'DisableResizing'
        $tabFriendly.Controls.Add($dgvA)
        $dgvA.Add_DataError({ $_.ThrowException = $false })

        $colANick    = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
        $colANick.HeaderText = "Nickname"; $colANick.Width = 130; $colANick.ReadOnly = $true; $colANick.Name = "Nickname"
        $colADefault = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
        $colADefault.HeaderText = "Set Default"; $colADefault.Width = 78; $colADefault.Name = "SetDefault"
        $colAVol     = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
        $colAVol.HeaderText = "Volume 0-100 (blank = don't change)"; $colAVol.Width = 250; $colAVol.Name = "Volume"
        [void]$dgvA.Columns.Add($colANick); [void]$dgvA.Columns.Add($colADefault); [void]$dgvA.Columns.Add($colAVol)

        foreach ($a in $prof.audio) {
            $volStr = if ($null -ne $a.volume) { [string]$a.volume } else { "" }
            [void]$dgvA.Rows.Add($a.nickname, [bool]$a.setDefault, $volStr)
        }

        # --- Start Processes DataGridView ---
        $lblProcSec = New-Object System.Windows.Forms.Label
        $lblProcSec.Text = "Start Processes"
        $lblProcSec.Font = New-Object System.Drawing.Font($editForm.Font.FontFamily, 9, [System.Drawing.FontStyle]::Bold)
        $lblProcSec.AutoSize = $true; $lblProcSec.Top = 312; $lblProcSec.Left = 6
        $tabFriendly.Controls.Add($lblProcSec)

        $dgvP = New-Object System.Windows.Forms.DataGridView
        $dgvP.Top = 332; $dgvP.Left = 4; $dgvP.Width = 720; $dgvP.Height = 112
        $dgvP.AllowUserToAddRows = $true; $dgvP.AllowUserToDeleteRows = $true
        $dgvP.RowHeadersVisible = $false; $dgvP.SelectionMode = 'FullRowSelect'
        $dgvP.AutoSizeColumnsMode = 'None'; $dgvP.EditMode = 'EditOnEnter'
        $dgvP.ColumnHeadersHeightSizeMode = 'DisableResizing'
        $tabFriendly.Controls.Add($dgvP)

        $colPPath  = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
        $colPPath.HeaderText = "Executable path"; $colPPath.Width = 320; $colPPath.Name = "Path"
        $colPArgs  = New-Object System.Windows.Forms.DataGridViewTextBoxColumn
        $colPArgs.HeaderText = "Arguments"; $colPArgs.Width = 180; $colPArgs.Name = "Args"
        $colPAdmin = New-Object System.Windows.Forms.DataGridViewCheckBoxColumn
        $colPAdmin.HeaderText = "As Admin"; $colPAdmin.Width = 62; $colPAdmin.Name = "AsAdmin"
        [void]$dgvP.Columns.Add($colPPath); [void]$dgvP.Columns.Add($colPArgs); [void]$dgvP.Columns.Add($colPAdmin)

        if ($prof.startProcesses) {
            foreach ($p in $prof.startProcesses) {
                [void]$dgvP.Rows.Add($p.path, $p.args, [bool]$p.asAdmin)
            }
        }

        # ── Tab 2: JSON editor ────────────────────────────────────────────
        $tabJson = New-Object System.Windows.Forms.TabPage; $tabJson.Text = "JSON (Advanced)"
        $outerTabs.TabPages.Add($tabJson)

        $lblJsonInfo = New-Object System.Windows.Forms.Label
        $lblJsonInfo.Text = "Direct JSON edit. Saving here does NOT sync back to the Profile Editor tab."
        $lblJsonInfo.AutoSize = $true; $lblJsonInfo.Top = 6; $lblJsonInfo.Left = 6
        $lblJsonInfo.ForeColor = [System.Drawing.Color]::DarkOrange
        $tabJson.Controls.Add($lblJsonInfo)

        $txtJson = New-Object System.Windows.Forms.TextBox
        $txtJson.Top = 28; $txtJson.Left = 4; $txtJson.Width = 722; $txtJson.Height = 460
        $txtJson.Multiline = $true; $txtJson.ScrollBars = 'Both'; $txtJson.WordWrap = $false
        $txtJson.Font = New-Object System.Drawing.Font("Consolas", 9)
        $txtJson.Anchor = [System.Windows.Forms.AnchorStyles]::Top -bor
                          [System.Windows.Forms.AnchorStyles]::Bottom -bor
                          [System.Windows.Forms.AnchorStyles]::Left -bor
                          [System.Windows.Forms.AnchorStyles]::Right
        $txtJson.Text = $prof | ConvertTo-Json -Depth 10
        $tabJson.Controls.Add($txtJson)

        # ── Bottom buttons ────────────────────────────────────────────────
        $btnSaveEdit = New-Object System.Windows.Forms.Button
        $btnSaveEdit.Text = "Save"
        $btnSaveEdit.Top = 524; $btnSaveEdit.Left = 8; $btnSaveEdit.Width = 100; $btnSaveEdit.Height = 32
        $btnSaveEdit.Anchor = [System.Windows.Forms.AnchorStyles]::Bottom -bor [System.Windows.Forms.AnchorStyles]::Left
        $editForm.Controls.Add($btnSaveEdit)

        $btnCancelEdit = New-Object System.Windows.Forms.Button
        $btnCancelEdit.Text = "Cancel"
        $btnCancelEdit.Top = 524; $btnCancelEdit.Left = 118; $btnCancelEdit.Width = 100; $btnCancelEdit.Height = 32
        $btnCancelEdit.Anchor = [System.Windows.Forms.AnchorStyles]::Bottom -bor [System.Windows.Forms.AnchorStyles]::Left
        $editForm.Controls.Add($btnCancelEdit)

        $btnSaveEdit.Add_Click({
            $dgvD.EndEdit(); $dgvA.EndEdit(); $dgvP.EndEdit()

            if ($outerTabs.SelectedIndex -eq 1) {
                # JSON tab
                try {
                    $parsed = $txtJson.Text | ConvertFrom-Json -ErrorAction Stop
                    $st3    = Get-State
                    if (-not $st3.profiles) { $st3 | Add-Member -NotePropertyName profiles -NotePropertyValue ([PSCustomObject]@{}) -Force }
                    $st3.profiles | Add-Member -NotePropertyName $profName -NotePropertyValue $parsed -Force
                    Save-State $st3
                    $editForm.DialogResult = [System.Windows.Forms.DialogResult]::OK
                    $editForm.Close(); Refresh-ProfileList
                } catch {
                    [System.Windows.Forms.MessageBox]::Show("Invalid JSON: $($_.Exception.Message)", "Error",
                        [System.Windows.Forms.MessageBoxButtons]::OK,
                        [System.Windows.Forms.MessageBoxIcon]::Error) | Out-Null
                }
            } else {
                # Friendly tab — reconstruct profile from DataGridViews
                $newDisp = @()
                foreach ($row in $dgvD.Rows) {
                    $nick = $row.Cells["Nickname"].Value; if (-not $nick) { continue }
                    $resVal = $row.Cells["Resolution"].Value
                    $w = $h = $hz = $null
                    if ($resVal -and $resVal -ne "(don't change)" -and $resVal -match '^(\d+)x(\d+)\s*@\s*(\d+)Hz$') {
                        $w = [uint32]$Matches[1]; $h = [uint32]$Matches[2]; $hz = [uint32]$Matches[3]
                    }
                    $dpiVal = $row.Cells["DPI"].Value
                    $dpiPct = $null
                    if ($dpiVal -and $dpiVal -ne "(don't change)" -and $dpiVal -match '^(\d+)%$') { $dpiPct = [uint32]$Matches[1] }
                    $hdrVal = $row.Cells["HDR"].Value
                    $hdrBool = if ($hdrVal -eq "On") { $true } elseif ($hdrVal -eq "Off") { $false } else { $null }
                    $moVal = $row.Cells["MirrorOf"].Value
                    $mo = if ($moVal -and $moVal -ne "(extend)" -and $moVal -ne $nick) { $moVal } else { $null }
                    $newDisp += [ordered]@{
                        nickname    = $nick
                        active      = [bool]$row.Cells["Active"].Value
                        primary     = [bool]$row.Cells["Primary"].Value
                        width       = $w; height = $h; refreshRate = $hz
                        dpiPercent  = $dpiPct; hdr = $hdrBool; rotation = $null
                        mirrorOf    = $mo
                    }
                }

                $newAudio = @()
                foreach ($row in $dgvA.Rows) {
                    $nick = $row.Cells["Nickname"].Value; if (-not $nick) { continue }
                    $volRaw = $row.Cells["Volume"].Value
                    $vol    = $null
                    if (-not [string]::IsNullOrWhiteSpace($volRaw)) {
                        if ($volRaw -notmatch '^\d+$' -or [int]$volRaw -gt 100) {
                            [System.Windows.Forms.MessageBox]::Show("Volume for '$nick' must be 0-100 or blank.") | Out-Null; return
                        }
                        $vol = [int]$volRaw
                    }
                    $newAudio += [ordered]@{ nickname = $nick; setDefault = [bool]$row.Cells["SetDefault"].Value; volume = $vol }
                }

                $newProcs = @()
                foreach ($row in $dgvP.Rows) {
                    if ($row.IsNewRow) { continue }
                    $path = $row.Cells["Path"].Value
                    if ([string]::IsNullOrWhiteSpace($path)) { continue }
                    $newProcs += [ordered]@{ path = $path; args = $row.Cells["Args"].Value; asAdmin = [bool]$row.Cells["AsAdmin"].Value }
                }

                $newProf = [ordered]@{ displays = $newDisp; audio = $newAudio; startProcesses = $newProcs }
                $st3 = Get-State
                if (-not $st3.profiles) { $st3 | Add-Member -NotePropertyName profiles -NotePropertyValue ([PSCustomObject]@{}) -Force }
                $st3.profiles | Add-Member -NotePropertyName $profName -NotePropertyValue $newProf -Force
                Save-State $st3
                $editForm.DialogResult = [System.Windows.Forms.DialogResult]::OK
                $editForm.Close(); Refresh-ProfileList
            }
        })
        $btnCancelEdit.Add_Click({ $editForm.Close() })
        [void]$editForm.ShowDialog($form)
    })

    $btnSave.Add_Click({
        $name = [Microsoft.VisualBasic.Interaction]::InputBox(
            "Name for the new profile (captures current live state):",
            "Save Profile", "")
        if ([string]::IsNullOrWhiteSpace($name)) { return }
        if ((Get-Profiles) -contains $name) {
            $ow = [System.Windows.Forms.MessageBox]::Show(
                "Profile '$name' exists. Overwrite?", "Confirm",
                [System.Windows.Forms.MessageBoxButtons]::YesNo,
                [System.Windows.Forms.MessageBoxIcon]::Warning)
            if ($ow -ne [System.Windows.Forms.DialogResult]::Yes) { return }
        }
        $outputBox.Clear()
        try {
            Save-ProfileInteractive -Name $name -ResolutionPrompt Gui *>&1 | ForEach-Object { $outputBox.AppendText("$_`r`n") }
        } catch { $outputBox.AppendText("ERROR: $($_.Exception.Message)`r`n") }
        Refresh-ProfileList
    })

    $btnDelete.Add_Click({
        if (-not $listBox.SelectedItem) { [System.Windows.Forms.MessageBox]::Show("Select a profile first.") | Out-Null; return }
        $name = $listBox.SelectedItem
        $conf = [System.Windows.Forms.MessageBox]::Show(
            "Delete profile '$name'?", "Confirm",
            [System.Windows.Forms.MessageBoxButtons]::YesNo,
            [System.Windows.Forms.MessageBoxIcon]::Warning)
        if ($conf -eq [System.Windows.Forms.DialogResult]::Yes) {
            Remove-Profile -Name $name
            Refresh-ProfileList
        }
    })

    $form.Add_Shown({ $form.Activate(); & $updateButtonStates })
    [void]$form.ShowDialog()
}

#endregion SECTION: GUI_Profile_Switcher

# =============================================================================
#region SECTION: CLI_Entry_Point
# =============================================================================

if ($Help -or ($RemainingArgs | Where-Object { $_ -match '^-{1,2}h(elp)?$' })) {
    Show-Help; return
}
elseif ($RemainingArgs -and $RemainingArgs.Count -gt 0) {
    Write-Host "Unrecognized argument(s): $($RemainingArgs -join ', ')" -ForegroundColor Red
    Write-Host "Run with -h or --help for usage." -ForegroundColor Yellow
    return
}

switch ($PSCmdlet.ParameterSetName) {
    'ApplyProfile' { Invoke-Profile -Name $Profile }
    'ListProfiles' { Show-Profiles }
    'ListDevices'  {
        Write-Section "Active Displays"
        Show-ActiveDisplays
        Write-Section "All Known Display Paths"
        Get-AllDisplayPaths | Where-Object { $_.FriendlyName } |
            Format-Table FriendlyName, Active, Primary, Width, Height, RefreshHz, DpiPercent, HdrEnabled, GdiDeviceName -AutoSize
        Write-Section "Audio Devices"
        Show-AudioDevices
        Write-Section "Registered Nicknames"
        $state = Get-State
        Write-Host "Displays:" -ForegroundColor Yellow
        if ($state.displays) {
            $state.displays.PSObject.Properties | ForEach-Object {
                Write-Host "  $($_.Name) -> $($_.Value.friendlyName) [adapter $($_.Value.adapterLUID), target $($_.Value.targetId)]"
            }
        }
        Write-Host "Audio:" -ForegroundColor Yellow
        if ($state.audio) {
            $state.audio.PSObject.Properties | ForEach-Object {
                Write-Host "  $($_.Name) -> $($_.Value.pattern) [$($_.Value.type)]"
            }
        }
        Write-Section "Duplicate Check"
        Test-DisplayNicknameDuplicates
        Test-AudioNicknameDuplicates
    }
    'Identify'      { Start-DeviceIdentificationWizard }
    'SetVolumeAll'  { Set-AllAudioVolume -Volume $SetVolumeAll }
    'SaveProfile'   { Save-ProfileInteractive -Name $SaveProfileAs }
    default         { Show-Tip; Show-ProfileSwitcherGui }
}

#endregion SECTION: CLI_Entry_Point