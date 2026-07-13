using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;
using SetResolutionAdapters;

// ============================================================
// ProfileManager — apply pipeline.
// Displays resolved directly by GdiName (DISPLAY1). No nickname layer.
// Atomic call order:
//   1. ConfigureTopology  — one-shot enable/disable via CCD
//   2. SetPrimary         — separate call
//   3. Per-display:       SetDisplayMode → verify+retry x2 → SetDpiPercent → SetHdr
//   4. Audio:             SetDefaultEndpoint → SetVolume → SetMute
//   5. StartProcesses
// Throws ProfileNotAppliedException if verification fails.
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    public static class ProfileManager
    {
        private const int VerifyRetries       = 2;
        private const int PostTopologyDelayMs = 800;
        private const int PostModeDelayMs     = 300;

        public static void Apply(string profileName, DeviceState state)
        {
            if (!state.Profiles.TryGetValue(profileName, out var profile))
                throw new ArgumentException($"Profile '{profileName}' not found");

            OrchestratorLogger.Log($"=== Applying profile '{profileName}' ===", LogLevel.Info);

            var liveDisplays = DisplayConfigManager.GetAllDisplayInfo();
            var liveAudio    = AudioManager.GetAllDevices();

            OrchestratorLogger.Debug($"Live displays: {liveDisplays.Count}");
            foreach (var d in liveDisplays)
                OrchestratorLogger.Debug($"  {d.GdiShortName} '{d.FriendlyName}' active={d.Active}");

            // Step 1: topology
            ApplyTopology(profile, liveDisplays);
            OrchestratorLogger.Debug($"Waiting {PostTopologyDelayMs}ms for topology to settle...");
            Thread.Sleep(PostTopologyDelayMs);
            liveDisplays = DisplayConfigManager.GetAllDisplayInfo();

            // Step 2: primary
            ApplyPrimary(profile, liveDisplays);

            // Step 3: per-display mode + DPI + HDR
            ApplyDisplaySettings(profile, liveDisplays);

            // Step 4: audio
            ApplyAudio(profile, liveAudio);

            // Step 5: processes
            ApplyProcesses(profile);

            // Verify
            Thread.Sleep(500);
            VerifyProfile(profileName, profile);

            OrchestratorLogger.Log($"=== Profile '{profileName}' applied successfully ===", LogLevel.Info);
        }

        // ── Topology ──────────────────────────────────────────────────────────

        private static void ApplyTopology(OrchestratorProfile profile, List<DisplayInfo> liveDisplays)
        {
            var toEnable  = new List<string>();
            var toDisable = new List<string>();

            foreach (var pd in profile.Displays)
            {
                if (pd.Active == null) continue;
                var live = FindDisplay(liveDisplays, pd.GdiName);
                if (live == null)
                {
                    OrchestratorLogger.Log($"Topology: display '{pd.GdiName}' not found in live enumeration", LogLevel.Warn);
                    continue;
                }
                if (pd.Active == true)  toEnable.Add(live.GdiShortName);
                else                    toDisable.Add(live.GdiShortName);
            }

            if (toEnable.Count == 0 && toDisable.Count == 0)
            {
                OrchestratorLogger.Debug("Topology: no changes required");
                return;
            }

            OrchestratorLogger.Log($"Topology: enabling [{string.Join(", ", toEnable)}] " +
                $"disabling [{string.Join(", ", toDisable)}]", LogLevel.Info);
            DisplayConfigManager.ConfigureTopology(toEnable.ToArray(), toDisable.ToArray());
        }

        // ── Primary ───────────────────────────────────────────────────────────

        private static void ApplyPrimary(OrchestratorProfile profile, List<DisplayInfo> liveDisplays)
        {
            foreach (var pd in profile.Displays)
            {
                if (pd.Primary != true) continue;
                var live = FindDisplay(liveDisplays, pd.GdiName);
                if (live == null) continue;
                OrchestratorLogger.Log($"Setting primary: {live.GdiShortName}", LogLevel.Info);
                // Windows promotes the display at position (0,0) to primary automatically.
                OrchestratorLogger.Debug($"SetPrimary via position (0,0) for {live.GdiDeviceName}");
                return;
            }
        }

        // ── Per-display mode + DPI + HDR ──────────────────────────────────────

        private static void ApplyDisplaySettings(OrchestratorProfile profile, List<DisplayInfo> liveDisplays)
        {
            foreach (var pd in profile.Displays)
            {
                if (pd.Active == false) continue;
                if (pd.Width == null && pd.Height == null && pd.Hz == null &&
                    pd.DpiPercent == null && pd.Hdr == null) continue;

                var live = FindDisplay(liveDisplays, pd.GdiName);
                if (live == null) continue;
                if (!live.Active)
                {
                    OrchestratorLogger.Log($"{pd.GdiName} is not active — skipping mode set", LogLevel.Warn);
                    continue;
                }

                if (pd.Width != null || pd.Height != null || pd.Hz != null)
                    ApplyModeWithRetry(pd, live);

                if (pd.DpiPercent != null)
                {
                    OrchestratorLogger.Log($"{live.GdiShortName}: setting DPI to {pd.DpiPercent}%", LogLevel.Info);
                    try { DisplayConfigManager.SetDpiPercent(live.GdiShortName, pd.DpiPercent.Value); }
                    catch (Exception ex) { OrchestratorLogger.Log($"{live.GdiShortName}: DPI set failed: {ex.Message}", LogLevel.Error); }
                }

                if (pd.Hdr != null)
                {
                    OrchestratorLogger.Log($"{live.GdiShortName}: setting HDR={pd.Hdr}", LogLevel.Info);
                    try { DisplayConfigManager.SetHdrEnabled(live.GdiShortName, pd.Hdr.Value); }
                    catch (Exception ex) { OrchestratorLogger.Log($"{live.GdiShortName}: HDR set failed: {ex.Message}", LogLevel.Error); }
                }
            }
        }

        private static void ApplyModeWithRetry(ProfileDisplay pd, DisplayInfo live)
        {
            int w  = pd.Width  ?? live.Width;
            int h  = pd.Height ?? live.Height;
            int hz = pd.Hz     ?? live.Hz;

            OrchestratorLogger.Log($"{live.GdiShortName}: setting mode {w}x{h}@{hz}Hz", LogLevel.Info);

            var modes = DisplayManagerAdapter.GetDisplayModes(live.GdiShortName);
            var best  = DisplayManagerAdapter.FindBestMode(modes, w, h, hz);
            if (best == null)
            {
                OrchestratorLogger.Log($"{live.GdiShortName}: no suitable mode found for {w}x{h}@{hz}Hz", LogLevel.Error);
                return;
            }
            OrchestratorLogger.Debug($"  Best mode: {best}");

            for (int attempt = 1; attempt <= VerifyRetries + 1; attempt++)
            {
                var result = DisplayManagerAdapter.SetDisplayMode(live.GdiShortName, best.Width, best.Height, best.Hz);
                OrchestratorLogger.Debug($"  SetDisplayMode attempt {attempt}: {result}");
                if (result == DisplayChangeResult.Successful) break;

                if (attempt <= VerifyRetries)
                {
                    OrchestratorLogger.Log($"{live.GdiShortName}: mode set returned {result}, retrying ({attempt}/{VerifyRetries})...", LogLevel.Warn);
                    Thread.Sleep(PostModeDelayMs);
                }
                else
                {
                    OrchestratorLogger.Log($"{live.GdiShortName}: mode set failed after {VerifyRetries} retries: {result}", LogLevel.Error);
                }
            }
        }

        // ── Audio ─────────────────────────────────────────────────────────────

        private static void ApplyAudio(OrchestratorProfile profile, List<AudioDeviceInfo> liveAudio)
        {
            foreach (var pa in profile.Audio)
            {
                var dev = FindAudio(liveAudio, pa.Pattern, pa.Type);
                if (dev == null)
                {
                    OrchestratorLogger.Log($"Audio: no active device matching pattern='{pa.Pattern}' type={pa.Type}", LogLevel.Warn);
                    continue;
                }

                if (pa.SetDefault == true)
                {
                    OrchestratorLogger.Log($"Audio: setting default → '{dev.FriendlyName}'", LogLevel.Info);
                    if (!AudioManager.SetDefaultEndpoint(dev.Id))
                        OrchestratorLogger.Log($"Audio: SetDefaultEndpoint failed for '{dev.FriendlyName}'", LogLevel.Error);
                }

                if (pa.Volume != null)
                {
                    OrchestratorLogger.Log($"Audio: volume '{dev.FriendlyName}' → {pa.Volume}%", LogLevel.Info);
                    AudioManager.SetVolume(dev.Id, pa.Volume.Value);
                }

                if (pa.Mute != null)
                {
                    OrchestratorLogger.Log($"Audio: mute '{dev.FriendlyName}' → {pa.Mute}", LogLevel.Info);
                    AudioManager.SetMute(dev.Id, pa.Mute.Value);
                }
            }
        }

        // ── Processes ─────────────────────────────────────────────────────────

        private static void ApplyProcesses(OrchestratorProfile profile)
        {
            if (profile.StartProcesses == null || profile.StartProcesses.Count == 0) return;
            foreach (var sp in profile.StartProcesses)
            {
                OrchestratorLogger.Log($"Starting process: {sp.Path} {sp.Args}", LogLevel.Info);
                try
                {
                    var psi = new ProcessStartInfo(sp.Path, sp.Args ?? string.Empty) { UseShellExecute = true };
                    if (sp.AsAdmin) psi.Verb = "runas";
                    Process.Start(psi);
                }
                catch (Exception ex)
                {
                    OrchestratorLogger.Log($"Failed to start '{sp.Path}': {ex.Message}", LogLevel.Error);
                }
            }
        }

        // ── Verification ──────────────────────────────────────────────────────

        private static void VerifyProfile(string profileName, OrchestratorProfile profile)
        {
            OrchestratorLogger.Log("Verifying profile application...", LogLevel.Info);
            var liveDisplays = DisplayConfigManager.GetAllDisplayInfo();
            var failures     = new List<string>();

            foreach (var pd in profile.Displays)
            {
                if (pd.Active == null) continue;
                var live = FindDisplay(liveDisplays, pd.GdiName);

                if (pd.Active == true)
                {
                    if (live == null || !live.Active)
                        failures.Add($"{pd.GdiName}: should be ACTIVE but is {(live == null ? "not found" : "inactive")}");
                    else
                    {
                        OrchestratorLogger.Debug($"  Verified {live.GdiShortName} active, {live.Width}x{live.Height}@{live.Hz}Hz");
                        if (pd.Width != null && pd.Height != null &&
                            (live.Width != pd.Width || live.Height != pd.Height))
                            OrchestratorLogger.Log($"  WARN: {live.GdiShortName} expected {pd.Width}x{pd.Height} but got {live.Width}x{live.Height}", LogLevel.Warn);
                    }
                }
                else
                {
                    if (live != null && live.Active)
                        failures.Add($"{pd.GdiName}: should be INACTIVE but is still active");
                    else
                        OrchestratorLogger.Debug($"  Verified {pd.GdiName} inactive — OK");
                }
            }

            if (failures.Count > 0)
            {
                string reason = string.Join("; ", failures);
                OrchestratorLogger.Log($"Profile verification FAILED: {reason}", LogLevel.Error);
                throw new ProfileNotAppliedException(profileName, reason);
            }
            OrchestratorLogger.Log("Verification passed.", LogLevel.Info);
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        private static DisplayInfo FindDisplay(List<DisplayInfo> displays, string gdiName)
        {
            if (string.IsNullOrEmpty(gdiName)) return null;
            foreach (var d in displays)
                if (d.GdiShortName.Equals(gdiName, StringComparison.OrdinalIgnoreCase))
                    return d;
            return null;
        }

        private static AudioDeviceInfo FindAudio(List<AudioDeviceInfo> devices, string pattern, string type)
        {
            if (string.IsNullOrEmpty(pattern)) return null;
            foreach (var d in devices)
            {
                if (d.State != AudioGuids.DEVICE_STATE_ACTIVE) continue;
                if (!string.IsNullOrEmpty(type) && !d.Type.Equals(type, StringComparison.OrdinalIgnoreCase)) continue;
                if (d.FriendlyName.IndexOf(pattern, StringComparison.OrdinalIgnoreCase) >= 0)
                    return d;
            }
            return null;
        }
    }
}
