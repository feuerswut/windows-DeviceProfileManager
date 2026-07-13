using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using SetResolutionAdapters;

// ============================================================
// DisplayConfigManager — high-level CCD operations.
// Port of the DisplayNative static methods from the PS1 reference
// (.old/DisplayAudioOrchestrator.ps1). Covers topology control,
// DPI, HDR — areas not handled by the SetResolution submodule
// (READ-ONLY — NOT TO BE EDITED UNDER ANY CIRCUMSTANCE).
// Display identification: GDI short name (DISPLAY1) via QueryDisplayConfig
// source name, then friendlyName as fallback.
// ============================================================

namespace DisplayAudioOrchestrator.CCD
{
    public sealed class DisplayInfo
    {
        public string GdiShortName   { get; set; }   // "DISPLAY1"
        public string GdiDeviceName  { get; set; }   // "\\.\DISPLAY1"
        public string FriendlyName   { get; set; }
        public bool   Active         { get; set; }
        public bool   Primary        { get; set; }
        public int    Width          { get; set; }
        public int    Height         { get; set; }
        public int    Hz             { get; set; }
        public int    PositionX      { get; set; }
        public int    PositionY      { get; set; }
        public int    DpiPercent     { get; set; }
        public bool   HdrEnabled     { get; set; }
        public bool   HdrSupported   { get; set; }
        internal LUID AdapterId      { get; set; }
        internal uint TargetId       { get; set; }
        internal uint SourceId       { get; set; }
    }

    public static class DisplayConfigManager
    {
        // ── Enumerate ─────────────────────────────────────────────────────────

        public static List<DisplayInfo> GetAllDisplayInfo()
        {
            uint numPaths, numModes;
            int rc = DisplayConfigNativeMethods.GetDisplayConfigBufferSizes(
                DisplayConfigFlags.QDC_ALL_PATHS, out numPaths, out numModes);
            if (rc != 0) throw new InvalidOperationException($"GetDisplayConfigBufferSizes failed: {rc}");

            var paths = new DISPLAYCONFIG_PATH_INFO[numPaths];
            var modes = new DISPLAYCONFIG_MODE_INFO[numModes];
            rc = DisplayConfigNativeMethods.QueryDisplayConfig(
                DisplayConfigFlags.QDC_ALL_PATHS, ref numPaths, paths, ref numModes, modes, IntPtr.Zero);
            if (rc != 0) throw new InvalidOperationException($"QueryDisplayConfig failed: {rc}");

            var result  = new List<DisplayInfo>();
            var seenSrc = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            foreach (var path in paths)
            {
                bool active = (path.flags & DisplayConfigFlags.DISPLAYCONFIG_PATH_ACTIVE) != 0;
                bool targetAvail = path.targetInfo.targetAvailable != 0;

                // Get GDI source name
                var srcName = new DISPLAYCONFIG_SOURCE_DEVICE_NAME
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type      = DisplayConfigFlags.DISP_INFO_GET_SOURCE_NAME,
                        size      = (uint)Marshal.SizeOf(typeof(DISPLAYCONFIG_SOURCE_DEVICE_NAME)),
                        adapterId = path.sourceInfo.adapterId,
                        id        = path.sourceInfo.id
                    }
                };
                DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref srcName);
                string gdiDevice = srcName.viewGdiDeviceName;   // "\\.\DISPLAY1"
                string gdiShort  = GdiShortName(gdiDevice);     // "DISPLAY1"

                if (seenSrc.Contains(gdiShort)) continue;
                seenSrc.Add(gdiShort);

                // Get friendly name
                var tgtName = new DISPLAYCONFIG_TARGET_DEVICE_NAME
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type      = DisplayConfigFlags.DISP_INFO_GET_TARGET_NAME,
                        size      = (uint)Marshal.SizeOf(typeof(DISPLAYCONFIG_TARGET_DEVICE_NAME)),
                        adapterId = path.targetInfo.adapterId,
                        id        = path.targetInfo.id
                    }
                };
                DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref tgtName);

                // Resolve resolution from source mode entry
                int w = 0, h = 0, hz = 0, px = 0, py = 0;
                uint srcModeIdx = path.sourceInfo.modeInfoIdx;
                if (srcModeIdx != 0xFFFFFFFF && srcModeIdx < numModes)
                {
                    var sm = modes[srcModeIdx].sourceMode;
                    w = (int)sm.width; h = (int)sm.height;
                    px = sm.position.x; py = sm.position.y;
                }
                uint tgtModeIdx = path.targetInfo.modeInfoIdx;
                if (tgtModeIdx != 0xFFFFFFFF && tgtModeIdx < numModes)
                {
                    var tm = modes[tgtModeIdx].targetMode.targetVideoSignalInfo;
                    if (tm.vSyncFreq.Denominator > 0)
                        hz = (int)Math.Round((double)tm.vSyncFreq.Numerator / tm.vSyncFreq.Denominator);
                }

                // DPI
                int dpi = 100;
                if (active)
                {
                    var dpiGet = new DpiConfigGet
                    {
                        header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                        {
                            type      = DisplayConfigFlags.DISP_INFO_GET_DPI_SCALE,
                            size      = (uint)Marshal.SizeOf(typeof(DpiConfigGet)),
                            adapterId = path.sourceInfo.adapterId,
                            id        = path.sourceInfo.id
                        }
                    };
                    if (DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref dpiGet) == 0)
                    {
                        int idx = (dpiGet.currentRelativeScale - dpiGet.minRelativeScale);
                        uint[] dpiVals = DisplayConfigFlags.DpiValues;
                        if (idx >= 0 && idx < dpiVals.Length) dpi = (int)dpiVals[idx];
                    }
                }

                // HDR
                bool hdrEnabled = false, hdrSupported = false;
                var hdrInfo = new AdvancedColorInfo2
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type      = DisplayConfigFlags.DISP_INFO_GET_ADVANCED_COLOR,
                        size      = (uint)Marshal.SizeOf(typeof(AdvancedColorInfo2)),
                        adapterId = path.targetInfo.adapterId,
                        id        = path.targetInfo.id
                    }
                };
                if (DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref hdrInfo) == 0)
                {
                    hdrEnabled   = hdrInfo.HDREnabled;
                    hdrSupported = hdrInfo.HDRSupported;
                }

                bool primary = px == 0 && py == 0 && active;

                result.Add(new DisplayInfo
                {
                    GdiShortName  = gdiShort,
                    GdiDeviceName = gdiDevice,
                    FriendlyName  = tgtName.monitorFriendlyDeviceName ?? gdiShort,
                    Active        = active,
                    Primary       = primary,
                    Width         = w,
                    Height        = h,
                    Hz            = hz,
                    PositionX     = px,
                    PositionY     = py,
                    DpiPercent    = dpi,
                    HdrEnabled    = hdrEnabled,
                    HdrSupported  = hdrSupported,
                    AdapterId     = path.targetInfo.adapterId,
                    TargetId      = path.targetInfo.id,
                    SourceId      = path.sourceInfo.id
                });
            }
            return result;
        }

        // ── Topology (enable / disable in one shot) ───────────────────────────

        // Enable all displays whose GdiShortName or FriendlyName matches any pattern in `enable`;
        // disable all others. Pass empty arrays to leave topology unchanged.
        public static void ConfigureTopology(string[] enablePatterns, string[] disablePatterns)
        {
            uint numPaths, numModes;
            int rc = DisplayConfigNativeMethods.GetDisplayConfigBufferSizes(
                DisplayConfigFlags.QDC_ALL_PATHS, out numPaths, out numModes);
            if (rc != 0) throw new InvalidOperationException($"GetDisplayConfigBufferSizes failed: {rc}");

            var paths = new DISPLAYCONFIG_PATH_INFO[numPaths];
            var modes = new DISPLAYCONFIG_MODE_INFO[numModes];
            rc = DisplayConfigNativeMethods.QueryDisplayConfig(
                DisplayConfigFlags.QDC_ALL_PATHS, ref numPaths, paths, ref numModes, modes, IntPtr.Zero);
            if (rc != 0) throw new InvalidOperationException($"QueryDisplayConfig failed: {rc}");

            var gdiNames = BuildGdiMap(paths, numPaths);

            for (int i = 0; i < numPaths; i++)
            {
                string gdi = gdiNames.ContainsKey(i) ? gdiNames[i] : string.Empty;

                bool shouldEnable  = enablePatterns  != null && enablePatterns.Length  > 0 && MatchesAny(gdi, enablePatterns);
                bool shouldDisable = disablePatterns != null && disablePatterns.Length > 0 && MatchesAny(gdi, disablePatterns);

                if (shouldEnable)
                    paths[i].flags |= DisplayConfigFlags.DISPLAYCONFIG_PATH_ACTIVE;
                else if (shouldDisable)
                    paths[i].flags &= ~DisplayConfigFlags.DISPLAYCONFIG_PATH_ACTIVE;
            }

            uint setFlags = DisplayConfigFlags.SDC_APPLY |
                            DisplayConfigFlags.SDC_USE_SUPPLIED_DISPLAY_CONFIG |
                            DisplayConfigFlags.SDC_SAVE_TO_DATABASE |
                            DisplayConfigFlags.SDC_ALLOW_CHANGES;
            rc = DisplayConfigNativeMethods.SetDisplayConfig(numPaths, paths, numModes, modes, setFlags);
            if (rc != 0) throw new InvalidOperationException($"SetDisplayConfig topology failed: {rc}");
        }

        // ── DPI ───────────────────────────────────────────────────────────────

        public static int GetDpiPercent(string gdiShortName)
        {
            var info = FindActiveInfo(gdiShortName);
            if (info == null) throw new ArgumentException($"Display not found or not active: {gdiShortName}");
            var req = new DpiConfigGet
            {
                header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                {
                    type      = DisplayConfigFlags.DISP_INFO_GET_DPI_SCALE,
                    size      = (uint)Marshal.SizeOf(typeof(DpiConfigGet)),
                    adapterId = info.AdapterId,
                    id        = info.SourceId
                }
            };
            int rc = DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref req);
            if (rc != 0) throw new InvalidOperationException($"DPI get failed: {rc}");
            int idx = req.currentRelativeScale - req.minRelativeScale;
            uint[] dpiVals = DisplayConfigFlags.DpiValues;
            return (idx >= 0 && idx < dpiVals.Length) ? (int)dpiVals[idx] : 100;
        }

        public static void SetDpiPercent(string gdiShortName, int dpiPercent)
        {
            var info = FindActiveInfo(gdiShortName);
            if (info == null) throw new ArgumentException($"Display not found or not active: {gdiShortName}");

            var reqGet = new DpiConfigGet
            {
                header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                {
                    type      = DisplayConfigFlags.DISP_INFO_GET_DPI_SCALE,
                    size      = (uint)Marshal.SizeOf(typeof(DpiConfigGet)),
                    adapterId = info.AdapterId,
                    id        = info.SourceId
                }
            };
            int rc = DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref reqGet);
            if (rc != 0) throw new InvalidOperationException($"DPI get (for set) failed: {rc}");

            uint[] dpiVals = DisplayConfigFlags.DpiValues;
            int bestIdx = 0;
            int bestDelta = int.MaxValue;
            for (int i = 0; i < dpiVals.Length; i++)
            {
                int d = Math.Abs((int)dpiVals[i] - dpiPercent);
                if (d < bestDelta) { bestDelta = d; bestIdx = i; }
            }
            int relativeScale = reqGet.minRelativeScale + bestIdx;

            var reqSet = new DpiConfigSet
            {
                header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                {
                    type      = DisplayConfigFlags.DISP_INFO_SET_DPI_SCALE,
                    size      = (uint)Marshal.SizeOf(typeof(DpiConfigSet)),
                    adapterId = info.AdapterId,
                    id        = info.SourceId
                },
                relativeScale = relativeScale
            };
            rc = DisplayConfigNativeMethods.DisplayConfigSetDeviceInfo(ref reqSet);
            if (rc != 0) throw new InvalidOperationException($"DPI set failed: {rc}");
        }

        // ── HDR ───────────────────────────────────────────────────────────────

        public static bool GetHdrEnabled(string gdiShortName)
        {
            var info = FindActiveInfo(gdiShortName);
            if (info == null) return false;
            var req = new AdvancedColorInfo2
            {
                header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                {
                    type      = DisplayConfigFlags.DISP_INFO_GET_ADVANCED_COLOR,
                    size      = (uint)Marshal.SizeOf(typeof(AdvancedColorInfo2)),
                    adapterId = info.AdapterId,
                    id        = info.TargetId
                }
            };
            DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref req);
            return req.HDREnabled;
        }

        public static void SetHdrEnabled(string gdiShortName, bool enabled)
        {
            var info = FindActiveInfo(gdiShortName);
            if (info == null) throw new ArgumentException($"Display not found or not active: {gdiShortName}");
            var req = new SetHdrState
            {
                header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                {
                    type      = DisplayConfigFlags.DISP_INFO_SET_HDR_STATE,
                    size      = (uint)Marshal.SizeOf(typeof(SetHdrState)),
                    adapterId = info.AdapterId,
                    id        = info.TargetId
                },
                value = enabled ? 1u : 0u
            };
            int rc = DisplayConfigNativeMethods.DisplayConfigSetDeviceInfo(ref req);
            if (rc != 0) throw new InvalidOperationException($"HDR set failed: {rc}");
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        private static DisplayInfo FindActiveInfo(string pattern)
        {
            var all = GetAllDisplayInfo();
            foreach (var d in all)
                if (d.Active && Matches(d, pattern)) return d;
            return null;
        }

        internal static bool Matches(DisplayInfo d, string pattern)
        {
            if (string.IsNullOrEmpty(pattern)) return false;
            return d.GdiShortName.Equals(pattern, StringComparison.OrdinalIgnoreCase)
                || (d.FriendlyName != null && d.FriendlyName.IndexOf(pattern, StringComparison.OrdinalIgnoreCase) >= 0);
        }

        private static bool MatchesAny(string gdiShort, string[] patterns)
        {
            foreach (var p in patterns)
                if (gdiShort.Equals(p, StringComparison.OrdinalIgnoreCase)) return true;
            return false;
        }

        private static Dictionary<int, string> BuildGdiMap(DISPLAYCONFIG_PATH_INFO[] paths, uint numPaths)
        {
            var map = new Dictionary<int, string>();
            for (int i = 0; i < (int)numPaths; i++)
            {
                var req = new DISPLAYCONFIG_SOURCE_DEVICE_NAME
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type      = DisplayConfigFlags.DISP_INFO_GET_SOURCE_NAME,
                        size      = (uint)Marshal.SizeOf(typeof(DISPLAYCONFIG_SOURCE_DEVICE_NAME)),
                        adapterId = paths[i].sourceInfo.adapterId,
                        id        = paths[i].sourceInfo.id
                    }
                };
                DisplayConfigNativeMethods.DisplayConfigGetDeviceInfo(ref req);
                map[i] = GdiShortName(req.viewGdiDeviceName);
            }
            return map;
        }

        private static string GdiShortName(string devicePath)
        {
            if (devicePath == null) return string.Empty;
            int slash = devicePath.LastIndexOf('\\');
            return slash >= 0 ? devicePath.Substring(slash + 1) : devicePath;
        }
    }
}
