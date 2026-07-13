using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

// ============================================================
// DisplayManagerAdapter — P/Invoke bridges for display enumeration and mode-setting.
// Mirrors the Win32 surface used by SetResolution's DisplayManagerNative (READ-ONLY
// submodule — NOT TO BE EDITED UNDER ANY CIRCUMSTANCE) but lives in our own
// adapter layer so we can extend behaviour without touching the submodule.
// Primary stable display identifier: GDI short name (DISPLAY1, DISPLAY2) from
// EnumDisplayDevices. FriendlyName is a fallback only.
// ============================================================

namespace SetResolutionAdapters
{
    // ── Public data classes ───────────────────────────────────────────────────

    public sealed class DisplayDeviceInfo
    {
        public int    Index      { get; internal set; }
        public string GdiName   { get; internal set; }   // "DISPLAY1"
        public string DeviceName { get; internal set; }  // "\\.\DISPLAY1"
        public string DeviceString { get; internal set; } // adapter name e.g. "NVIDIA GeForce RTX 4090"
        public bool   IsPrimary { get; internal set; }
        public bool   IsActive  { get; internal set; }
        public string MonitorName { get; internal set; } // friendly name from monitor DISPLAY_DEVICE
    }

    public sealed class DisplayModeInfo
    {
        public int Width     { get; set; }
        public int Height    { get; set; }
        public int Hz        { get; set; }
        public int BitDepth  { get; set; }

        public override string ToString() => $"{Width}x{Height}@{Hz}Hz {BitDepth}bpp";
    }

    public enum DisplayChangeResult
    {
        Successful             =  0,
        Restart                =  1,
        Failed                 = -1,
        BadMode                = -2,
        NotUpdated             = -3,
        BadFlags               = -4,
        BadParam               = -5,
        BadDualView            = -6
    }

    // ── P/Invoke types (own copies — do not share with submodule internals) ──

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Ansi)]
    internal struct DISPLAY_DEVICE
    {
        public int    cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]  public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        public uint   StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Ansi)]
    internal struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public ushort dmSpecVersion;
        public ushort dmDriverVersion;
        public ushort dmSize;
        public ushort dmDriverExtra;
        public uint   dmFields;
        public int    dmPositionX;
        public int    dmPositionY;
        public uint   dmDisplayOrientation;
        public uint   dmDisplayFixedOutput;
        public short  dmColor;
        public short  dmDuplex;
        public short  dmYResolution;
        public short  dmTTOption;
        public short  dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public ushort dmLogPixels;
        public uint   dmBitsPerPel;
        public uint   dmPelsWidth;
        public uint   dmPelsHeight;
        public uint   dmDisplayFlags;
        public uint   dmDisplayFrequency;
        public uint   dmICMMethod;
        public uint   dmICMIntent;
        public uint   dmMediaType;
        public uint   dmDitherType;
        public uint   dmReserved1;
        public uint   dmReserved2;
        public uint   dmPanningWidth;
        public uint   dmPanningHeight;
    }

    internal static class NativeMethods
    {
        internal const uint DISPLAY_DEVICE_PRIMARY_DEVICE  = 0x00000004;
        internal const uint DISPLAY_DEVICE_ACTIVE          = 0x00000001;
        internal const uint DISPLAY_DEVICE_ATTACHED        = 0x00000002;

        internal const uint DM_PELSWIDTH   = 0x00080000;
        internal const uint DM_PELSHEIGHT  = 0x00100000;
        internal const uint DM_DISPLAYFREQ = 0x00400000;

        internal const uint CDS_UPDATEREGISTRY   = 0x00000001;
        internal const uint ENUM_CURRENT_SETTINGS = 0xFFFFFFFF;
        internal const uint ENUM_REGISTRY_SETTINGS = 0xFFFFFFFE;

        internal const int DISP_CHANGE_SUCCESSFUL = 0;

        [DllImport("user32.dll", CharSet = CharSet.Ansi)]
        internal static extern bool EnumDisplayDevices(string lpDevice, uint iDevNum,
            ref DISPLAY_DEVICE lpDisplayDevice, uint dwFlags);

        [DllImport("user32.dll", CharSet = CharSet.Ansi)]
        internal static extern bool EnumDisplaySettings(string lpszDeviceName, uint iModeNum,
            ref DEVMODE lpDevMode);

        [DllImport("user32.dll", CharSet = CharSet.Ansi)]
        internal static extern int ChangeDisplaySettingsEx(string lpszDeviceName,
            ref DEVMODE lpDevMode, IntPtr hwnd, uint dwFlags, IntPtr lParam);
    }

    // ── Public adapter API ────────────────────────────────────────────────────

    public static class DisplayManagerAdapter
    {
        // Returns all display adapters in Windows enumeration order.
        // GdiName ("DISPLAY1") is the primary stable key.
        public static List<DisplayDeviceInfo> GetAllDisplayDevices()
        {
            var result = new List<DisplayDeviceInfo>();
            uint i = 0;
            while (true)
            {
                var adapter = new DISPLAY_DEVICE { cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE)) };
                if (!NativeMethods.EnumDisplayDevices(null, i, ref adapter, 0)) break;

                bool active  = (adapter.StateFlags & NativeMethods.DISPLAY_DEVICE_ACTIVE)          != 0;
                bool primary = (adapter.StateFlags & NativeMethods.DISPLAY_DEVICE_PRIMARY_DEVICE)  != 0;

                // Probe child monitor device for friendly name
                var mon = new DISPLAY_DEVICE { cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE)) };
                NativeMethods.EnumDisplayDevices(adapter.DeviceName, 0, ref mon, 0);

                string gdiShort = GdiShortName(adapter.DeviceName); // "DISPLAY1"

                result.Add(new DisplayDeviceInfo
                {
                    Index        = (int)i + 1,
                    GdiName      = gdiShort,
                    DeviceName   = adapter.DeviceName,
                    DeviceString = adapter.DeviceString,
                    IsPrimary    = primary,
                    IsActive     = active,
                    MonitorName  = string.IsNullOrEmpty(mon.DeviceString) ? adapter.DeviceString : mon.DeviceString
                });
                i++;
            }
            return result;
        }

        // Returns all available modes for the given adapter (by GDI short name or full path).
        public static List<DisplayModeInfo> GetDisplayModes(string gdiName)
        {
            string devicePath = ResolveDevicePath(gdiName);
            var result = new List<DisplayModeInfo>();
            uint modeNum = 0;
            while (true)
            {
                var dm = new DEVMODE { dmSize = (ushort)Marshal.SizeOf(typeof(DEVMODE)) };
                if (!NativeMethods.EnumDisplaySettings(devicePath, modeNum, ref dm)) break;
                result.Add(new DisplayModeInfo
                {
                    Width    = (int)dm.dmPelsWidth,
                    Height   = (int)dm.dmPelsHeight,
                    Hz       = (int)dm.dmDisplayFrequency,
                    BitDepth = (int)dm.dmBitsPerPel
                });
                modeNum++;
            }
            return result;
        }

        // Returns the currently active mode for the given adapter.
        public static DisplayModeInfo GetCurrentMode(string gdiName)
        {
            string devicePath = ResolveDevicePath(gdiName);
            var dm = new DEVMODE { dmSize = (ushort)Marshal.SizeOf(typeof(DEVMODE)) };
            if (!NativeMethods.EnumDisplaySettings(devicePath, NativeMethods.ENUM_CURRENT_SETTINGS, ref dm))
                return null;
            return new DisplayModeInfo
            {
                Width    = (int)dm.dmPelsWidth,
                Height   = (int)dm.dmPelsHeight,
                Hz       = (int)dm.dmDisplayFrequency,
                BitDepth = (int)dm.dmBitsPerPel
            };
        }

        // Atomically sets width/height/hz on the named adapter. Uses CDS_UPDATEREGISTRY.
        public static DisplayChangeResult SetDisplayMode(string gdiName, int width, int height, int hz)
        {
            string devicePath = ResolveDevicePath(gdiName);
            var dm = new DEVMODE { dmSize = (ushort)Marshal.SizeOf(typeof(DEVMODE)) };
            NativeMethods.EnumDisplaySettings(devicePath, NativeMethods.ENUM_CURRENT_SETTINGS, ref dm);

            dm.dmPelsWidth        = (uint)width;
            dm.dmPelsHeight       = (uint)height;
            dm.dmDisplayFrequency = (uint)hz;
            dm.dmFields           = NativeMethods.DM_PELSWIDTH | NativeMethods.DM_PELSHEIGHT | NativeMethods.DM_DISPLAYFREQ;

            int r = NativeMethods.ChangeDisplaySettingsEx(devicePath, ref dm, IntPtr.Zero,
                        NativeMethods.CDS_UPDATEREGISTRY, IntPtr.Zero);
            return (DisplayChangeResult)r;
        }

        // Fuzzy best-mode finder: exact match → same-AR closest height → closest Hz.
        // hzTolerance: how far Hz may deviate (0 = exact).
        public static DisplayModeInfo FindBestMode(List<DisplayModeInfo> modes,
            int targetW, int targetH, int targetHz, int hzTolerance = 1)
        {
            // 1. Exact
            foreach (var m in modes)
                if (m.Width == targetW && m.Height == targetH &&
                    Math.Abs(m.Hz - targetHz) <= hzTolerance) return m;

            // 2. Same AR, closest height
            double ar = (double)targetW / targetH;
            DisplayModeInfo best = null;
            int bestDeltaH = int.MaxValue, bestDeltaHz = int.MaxValue;
            foreach (var m in modes)
            {
                if (Math.Abs((double)m.Width / m.Height - ar) > 0.01) continue;
                int dh = Math.Abs(m.Height - targetH);
                int dhz = Math.Abs(m.Hz - targetHz);
                if (dh < bestDeltaH || (dh == bestDeltaH && dhz < bestDeltaHz))
                { best = m; bestDeltaH = dh; bestDeltaHz = dhz; }
            }
            if (best != null) return best;

            // 3. Closest Hz regardless of resolution
            bestDeltaHz = int.MaxValue;
            foreach (var m in modes)
            {
                int dhz = Math.Abs(m.Hz - targetHz);
                if (dhz < bestDeltaHz) { best = m; bestDeltaHz = dhz; }
            }
            return best;
        }

        // ── Helpers ──────────────────────────────────────────────────────────

        // "\\.\DISPLAY1" → "DISPLAY1" (drop the "\\.\" prefix)
        private static string GdiShortName(string devicePath)
        {
            if (devicePath == null) return string.Empty;
            int lastSlash = devicePath.LastIndexOf('\\');
            return lastSlash >= 0 ? devicePath.Substring(lastSlash + 1) : devicePath;
        }

        // Accept "DISPLAY1" or "\\.\DISPLAY1" — return full path.
        private static string ResolveDevicePath(string gdiName)
        {
            if (gdiName == null) return null;
            if (gdiName.StartsWith("\\\\.\\")) return gdiName;
            // Try to find it by enum to get canonical path
            uint i = 0;
            while (true)
            {
                var dd = new DISPLAY_DEVICE { cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE)) };
                if (!NativeMethods.EnumDisplayDevices(null, i, ref dd, 0)) break;
                string sh = GdiShortName(dd.DeviceName);
                if (sh.Equals(gdiName, StringComparison.OrdinalIgnoreCase) ||
                    dd.DeviceName.Equals(gdiName, StringComparison.OrdinalIgnoreCase))
                    return dd.DeviceName;
                i++;
            }
            return $"\\\\.\\{gdiName}"; // best-effort fallback
        }
    }
}
