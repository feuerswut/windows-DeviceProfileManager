using System;
using System.Runtime.InteropServices;

// ============================================================
// DisplayConfigNative — structs and P/Invoke for the Windows CCD API.
// Verbatim port of the DisplayNative Add-Type block from the PS1 reference
// script (.old/DisplayAudioOrchestrator.ps1, lines 121-743).
// Covers enable/disable/topology operations that are NOT in the SetResolution
// submodule (READ-ONLY — NOT TO BE EDITED UNDER ANY CIRCUMSTANCE).
// ============================================================

namespace DisplayAudioOrchestrator.CCD
{
    internal static class DisplayConfigFlags
    {
        public const uint QDC_ALL_PATHS          = 0x00000001;
        public const uint QDC_ONLY_ACTIVE_PATHS  = 0x00000002;
        public const uint QDC_VIRTUAL_MODE_AWARE = 0x00000010;

        public const uint SDC_TOPOLOGY_SUPPLIED           = 0x00000010;
        public const uint SDC_USE_SUPPLIED_DISPLAY_CONFIG = 0x00000020;
        public const uint SDC_APPLY                       = 0x00000080;
        public const uint SDC_SAVE_TO_DATABASE            = 0x00000200;
        public const uint SDC_ALLOW_CHANGES               = 0x00000400;
        public const uint SDC_ALLOW_PATH_ORDER_CHANGES    = 0x00002000;

        public const uint DISPLAYCONFIG_PATH_ACTIVE = 0x00000001;

        public const int DISP_INFO_GET_SOURCE_NAME    = 1;
        public const int DISP_INFO_GET_TARGET_NAME    = 2;
        public const int DISP_INFO_GET_ADVANCED_COLOR = 15;
        public const int DISP_INFO_SET_HDR_STATE      = 16;
        public const int DISP_INFO_GET_DPI_SCALE      = -3;
        public const int DISP_INFO_SET_DPI_SCALE      = -4;

        public static readonly uint[] DpiValues = { 100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500 };
    }

    // ── Core structs ──────────────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    internal struct LUID
    {
        public uint LowPart;
        public int  HighPart;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct POINTL
    {
        public int x;
        public int y;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct RECTL
    {
        public int left, top, right, bottom;
    }

    // ── Device info header (base for all DisplayConfig*Get/Set structs) ───────

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_DEVICE_INFO_HEADER
    {
        public int  type;
        public uint size;
        public LUID adapterId;
        public uint id;
    }

    // ── Device name query structs ─────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    internal struct DISPLAYCONFIG_TARGET_DEVICE_NAME
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
    internal struct DISPLAYCONFIG_SOURCE_DEVICE_NAME
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string viewGdiDeviceName;
    }

    // ── Path info ─────────────────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_PATH_SOURCE_INFO
    {
        public LUID adapterId;
        public uint id;
        public uint modeInfoIdx;
        public uint statusFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_PATH_TARGET_INFO
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
    internal struct DISPLAYCONFIG_PATH_INFO
    {
        public DISPLAYCONFIG_PATH_SOURCE_INFO sourceInfo;
        public DISPLAYCONFIG_PATH_TARGET_INFO targetInfo;
        public uint flags;
    }

    // ── Mode info union ───────────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_2DREGION { public uint cx; public uint cy; }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_RATIONAL { public uint Numerator; public uint Denominator; }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_VIDEO_SIGNAL_INFO
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
    internal struct DISPLAYCONFIG_TARGET_MODE
    {
        public DISPLAYCONFIG_VIDEO_SIGNAL_INFO targetVideoSignalInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_SOURCE_MODE
    {
        public uint   width;
        public uint   height;
        public uint   pixelFormat;
        public POINTL position;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DISPLAYCONFIG_DESKTOP_IMAGE_INFO
    {
        public POINTL PathSourceSize;
        public RECTL  DesktopImageRegion;
        public RECTL  DesktopImageClip;
    }

    [StructLayout(LayoutKind.Explicit, Size = 64)]
    internal struct DISPLAYCONFIG_MODE_INFO
    {
        [FieldOffset( 0)] public uint infoType;
        [FieldOffset( 4)] public uint id;
        [FieldOffset( 8)] public LUID adapterId;
        [FieldOffset(16)] public DISPLAYCONFIG_TARGET_MODE      targetMode;
        [FieldOffset(16)] public DISPLAYCONFIG_SOURCE_MODE      sourceMode;
        [FieldOffset(16)] public DISPLAYCONFIG_DESKTOP_IMAGE_INFO desktopImageInfo;
    }

    // ── DPI query/set structs ─────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    internal struct DpiConfigGet
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public int minRelativeScale;
        public int currentRelativeScale;
        public int maxRelativeScale;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct DpiConfigSet
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public int relativeScale;
    }

    // ── HDR / Advanced Color structs ──────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    internal struct AdvancedColorInfo2
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint value;
        public bool HDRSupported  { get { return (value & 0x10u) != 0; } }
        public bool HDREnabled    { get { return (value & 0x20u) != 0; } }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SetHdrState
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint value;
    }

    // ── P/Invoke ──────────────────────────────────────────────────────────────

    internal static class DisplayConfigNativeMethods
    {
        [DllImport("user32.dll")]
        public static extern int GetDisplayConfigBufferSizes(uint flags, out uint numPaths, out uint numModes);

        [DllImport("user32.dll")]
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
        public static extern int DisplayConfigGetDeviceInfo(ref AdvancedColorInfo2 req);

        [DllImport("user32.dll")]
        public static extern int DisplayConfigSetDeviceInfo(ref DpiConfigSet req);

        [DllImport("user32.dll")]
        public static extern int DisplayConfigSetDeviceInfo(ref SetHdrState req);

        [DllImport("kernel32.dll")]
        public static extern IntPtr GetConsoleWindow();

        [DllImport("user32.dll")]
        public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        public const int SW_HIDE = 0;
        public const int SW_SHOW = 5;
    }
}
