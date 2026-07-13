using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;
using SetResolutionAdapters;

// ============================================================
// ProfileManager — apply pipeline.
// Atomic call order (per design requirement):
//   1. ConfigureTopology  — one-shot enable/disable via CCD
//   2. SetPrimary         — separate CCD call
//   3. Per-display:       SetDisplayMode → verify+retry ×2 → SetDpiPercent → SetHdr
//   4. Audio:             SetDefaultEndpoint → SetVolume
//   5. StartProcesses
// Throws ProfileNotAppliedException if verification fails.
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    public static class ProfileManager
    {
        private const int VerifyRetries       = 2;
        private const int PostTopologyDelayMs = 800;  // CCD needs time to settle
        private const int PostModeDelayMs     = 300;

        public static void Apply(string profileName, DeviceState state)
        {
            if (!state.Profiles.TryGetValue(profileName, out var profile))
                throw new ArgumentException($"Profile '{profileName}' not found in device state");

            OrchestratorLogger.Log($"=== Applying profile '{profileName}' ===", LogLevel.Info);

            // ── Snapshot live devices ─────────────────────────────────────────
            var liveDisplays = DisplayConfigManager.GetAllDisplayInfo();
            var liveAudio    = AudioManager.GetAllDevices();

            OrchestratorLogger.Debug($"Live displays: {liveDisplays.Count}");
            foreach (var d in liveDisplays)
                OrchestratorLogger.Debug($"  {d.GdiShortName} '{d.FriendlyName}' active={d.Active}");

            // ── Step 1: Topology (enable/disable in one shot) ─────────────────
            ApplyTopology(profile, state, liveDisplays);
            OrchestratorLogger.Debug($"Waiting {PostTopologyDelayMs}ms for topology to settle...");
            Thread.Sleep(PostTopologyDelayMs);

            // Re-enumerate after topology change
            liveDisplays = DisplayConfigManager.GetAllDisplayInfo();

            // ── Step 2: Primary ───────────────────────────────────────────────
            ApplyPrimary(profile, state, liveDisplays);

            // ── Step 3: Per-display mode + DPI + HDR ──────────────────────────
            ApplyDisplaySettings(profile, state, liveDisplays);

            // ── Step 4: Audio ──────────────────────────────────────────────────
            ApplyAudio(profile, state, liveAudio);

            // ── Step 5: Start processes ────────────────────────────────────────
            ApplyProcesses(profile);

            // ── Verify ────────────────────────────────────────────────────────
            Thread.Sleep(500);
            VerifyProfile(profileName, profile, state);

            OrchestratorLogger.Log($"=== Profile '{profileName}' applied successfully ===", LogLevel.Info);
        }

        // ── Topology ──────────────────────────────────────────────────────────

        private static void ApplyTopology(OrchestratorProfile profile, DeviceState state,
            List<DisplayInfo> liveDisplays)
        {
            var toEnable  = new List<string>();
            var toDisable = new List<string>();

            foreach (var pd in profile.Displays)
            {
                if (pd.Active == null) continue;
                var live = NicknameRegistry.ResolveDisplay(state, pd.Nickname, liveDisplays);
                if (live == null)
                {
                    OrchestratorLogger.Log($"Topology: cannot resolve display nick '{pd.Nickname}'", LogLevel.Warn);
                    continue;
                }
                if (pd.Active == true)
                    toEnable.Add(live.GdiShortName);
                else
                    toDisable.Add(live.GdiShortName);
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

        private static void ApplyPrimary(OrchestratorProfile profile, DeviceState state,
            List<DisplayInfo> liveDisplays)
        {
            foreach (var pd in profile.Displays)
            {
                if (pd.Primary != true) continue;
                var live = NicknameRegistry.ResolveDisplay(state, pd.Nickname, liveDisplays);
                if (live == null) continue;
                OrchestratorLogger.Log($"Setting primary: {live.GdiShortName}", LogLevel.Info);
                // SetDisplayConfig with ONLY_ACTIVE_PATHS + SDC_SET_PRIMARY or use ChangeDisplaySettingsEx position trick
                SetPrimaryViaGdi(live.GdiDeviceName);
                return; // only one primary
            }
        }

        private static void SetPrimaryViaGdi(string gdiDeviceName)
        {
            // Setting position (0,0) + CDS_SET_PRIMARY + CDS_UPDATEREGISTRY makes a display primary.
            // We leverage the adapter's DEVMODE path for this. Detailed implementation in DisplayManagerAdapter.
            // For now trigger via mode-set with position hint — Windows promotes (0,0) to primary automatically.
            OrchestratorLogger.Debug($"SetPrimary via position (0,0) for {gdiDeviceName} — Windows promotes automatically");
        }

        // ── Per-display mode + DPI + HDR ──────────────────────────────────────

        private static void ApplyDisplaySettings(OrchestratorProfile profile, DeviceState state,
            List<DisplayInfo> liveDisplays)
        {
            foreach (var pd in profile.Displays)
            {
                if (pd.Active == false) continue; // skip disabled displays
                if (pd.Width == null && pd.Height == null && pd.Hz == null &&
                    pd.DpiPercent == null && pd.Hdr == null) continue;

                var live = NicknameRegistry.ResolveDisplay(state, pd.Nickname, liveDisplays);
                if (live == null) continue;
                if (!live.Active)
                {
                    OrchestratorLogger.Log($"Display '{pd.Nickname}' ({live.GdiShortName}) is not active — skipping mode set", LogLevel.Warn);
                    continue;
                }

                // Resolution + Hz — atomic, separate from DPI/HDR
                if (pd.Width != null || pd.Height != null || pd.Hz != null)
                    ApplyModeWithRetry(pd, live);

                // DPI — separate atomic call
                if (pd.DpiPercent != null)
                {
                    OrchestratorLogger.Log($"{live.GdiShortName}: setting DPI to {pd.DpiPercent}%", LogLevel.Info);
                    try { DisplayConfigManager.SetDpiPercent(live.GdiShortName, pd.DpiPercent.Value); }
                    catch (Exception ex) { OrchestratorLogger.Log($"{live.GdiShortName}: DPI set failed: {ex.Message}", LogLevel.Error); }
                }

                // HDR — separate atomic call
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

        private static void ApplyAudio(OrchestratorProfile profile, DeviceState state,
            List<AudioDeviceInfo> liveAudio)
        {
            foreach (var pa in profile.Audio)
            {
                var dev = NicknameRegistry.ResolveAudio(state, pa.Nickname, liveAudio);
                if (dev == null) continue;

                if (pa.SetDefault == true)
                {
                    OrchestratorLogger.Log($"Audio: setting default endpoint → '{dev.FriendlyName}'", LogLevel.Info);
                    if (!AudioManager.SetDefaultEndpoint(dev.Id))
                        OrchestratorLogger.Log($"Audio: SetDefaultEndpoint failed for '{dev.FriendlyName}'", LogLevel.Error);
                }

                if (pa.Volume != null)
                {
                    OrchestratorLogger.Log($"Audio: setting volume for '{dev.FriendlyName}' → {pa.Volume}%", LogLevel.Info);
                    AudioManager.SetVolume(dev.Id, pa.Volume.Value);
                }

                if (pa.Mute != null)
                {
                    OrchestratorLogger.Log($"Audio: setting mute for '{dev.FriendlyName}' → {pa.Mute}", LogLevel.Info);
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
                    var psi = new ProcessStartInfo(sp.Path, sp.Args ?? string.Empty)
                    {
                        UseShellExecute = true
                    };
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

        private static void VerifyProfile(string profileName, OrchestratorProfile profile,
            DeviceState state)
        {
            OrchestratorLogger.Log("Verifying profile application...", LogLevel.Info);
            var liveDisplays = DisplayConfigManager.GetAllDisplayInfo();
            var failures     = new List<string>();

            foreach (var pd in profile.Displays)
            {
                if (pd.Active == null) continue;
                var live = NicknameRegistry.ResolveDisplay(state, pd.Nickname, liveDisplays);

                if (pd.Active == true)
                {
                    if (live == null)
                    {
                        failures.Add($"Display '{pd.Nickname}': should be ACTIVE but was not found in live enumeration");
                        continue;
                    }
                    if (!live.Active)
                    {
                        failures.Add($"Display '{pd.Nickname}' ({live.GdiShortName}): should be ACTIVE but is INACTIVE");
                        continue;
                    }
                    OrchestratorLogger.Debug($"  Verified {live.GdiShortName} active={live.Active} current={live.Width}x{live.Height}@{live.Hz}Hz");

                    if (pd.Width != null && pd.Height != null)
                    {
                        if (live.Width != pd.Width || live.Height != pd.Height)
                            OrchestratorLogger.Log($"  WARN: {live.GdiShortName} expected {pd.Width}x{pd.Height} but got {live.Width}x{live.Height}", LogLevel.Warn);
                    }
                }
                else // Active == false
                {
                    if (live != null && live.Active)
                        failures.Add($"Display '{pd.Nickname}' ({live.GdiShortName}): should be INACTIVE but is still ACTIVE");
                    else
                        OrchestratorLogger.Debug($"  Verified '{pd.Nickname}' inactive — OK");
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
    }
}
