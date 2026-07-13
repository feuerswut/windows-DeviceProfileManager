using System;
using System.Collections.Generic;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.GUI;
using DisplayAudioOrchestrator.Orchestrator;

namespace DisplayAudioOrchestrator
{
    public static class OrchestratorProcessor
    {
        public static void Process(OrchestratorOptions opts)
        {
            if (opts.Debug)
                OrchestratorLogger.DebugMode = true;

            if (opts.Help)
            {
                OrchestratorCommandLineParser.PrintHelp();
                return;
            }

            if (opts.ListDevices)
                RunListDevices();

            if (opts.ListProfiles)
                RunListProfiles();

            if (opts.Identify)
                RunIdentify();

            if (opts.ApplyProfile != null)
                RunApplyProfile(opts.ApplyProfile);

            if (opts.SetVolumeAll != null)
                RunSetVolumeAll(opts.SetVolumeAll.Value);
        }

        // ── List devices ──────────────────────────────────────────────────────

        private static void RunListDevices()
        {
            Console.WriteLine("\n── Displays ──────────────────────────────────────────");
            var displays = DisplayConfigManager.GetAllDisplayInfo();
            foreach (var d in displays)
            {
                Console.WriteLine($"  {d.GdiShortName,-10} {(d.Active ? "ACTIVE " : "inactive")} " +
                    $"{(d.Primary ? "[PRIMARY]" : "         ")} " +
                    $"{d.Width}x{d.Height}@{d.Hz}Hz DPI:{d.DpiPercent}% " +
                    $"HDR:{(d.HdrEnabled ? "ON" : "off")} " +
                    $"'{d.FriendlyName}'");
            }

            Console.WriteLine("\n── Audio ─────────────────────────────────────────────");
            var audio = AudioManager.GetAllDevices();
            foreach (var a in audio)
            {
                Console.WriteLine($"  [{a.Type,-9}] {(a.IsDefault ? "[DEFAULT]" : "         ")} " +
                    $"{(a.State == AudioGuids.DEVICE_STATE_ACTIVE ? "ACTIVE " : "inactive")} " +
                    $"Vol:{(a.VolumePercent >= 0 ? a.VolumePercent + "%" : "n/a"),-5} " +
                    $"'{a.FriendlyName}'");
            }
            Console.WriteLine();
        }

        // ── List profiles ─────────────────────────────────────────────────────

        private static void RunListProfiles()
        {
            var state = StateStore.Load();
            Console.WriteLine("\n── Profiles ─────────────────────────────────────────");
            if (state.Profiles.Count == 0)
            {
                Console.WriteLine("  (none)");
                Console.WriteLine();
                return;
            }
            foreach (var kv in state.Profiles)
            {
                var p = kv.Value;
                Console.WriteLine($"  {kv.Key}");
                foreach (var d in p.Displays)
                    Console.WriteLine($"    display {d.GdiName} active={d.Active} {d.Width}x{d.Height}@{d.Hz}Hz dpi={d.DpiPercent} hdr={d.Hdr}");
                foreach (var a in p.Audio)
                    Console.WriteLine($"    audio   '{a.Pattern}' [{a.Type}] default={a.SetDefault} vol={a.Volume}");
            }
            Console.WriteLine();
        }

        // ── Identify ──────────────────────────────────────────────────────────

        private static void RunIdentify()
        {
            // Must run on STA thread for WinForms
            System.Threading.Thread sta = new System.Threading.Thread(() =>
            {
                System.Windows.Forms.Application.EnableVisualStyles();
                MonitorOverlayForm.ShowOverlays();
            });
            sta.SetApartmentState(System.Threading.ApartmentState.STA);
            sta.Start();
            sta.Join();
        }

        // ── Apply profile ─────────────────────────────────────────────────────

        private static void RunApplyProfile(string profileName)
        {
            OrchestratorLogger.Log($"Applying profile: {profileName}", LogLevel.Info);
            var state = StateStore.Load();
            try
            {
                ProfileManager.Apply(profileName, state);
            }
            catch (ProfileNotAppliedException ex)
            {
                OrchestratorLogger.Log(ex.Message, LogLevel.Error);
                Environment.Exit(1);
            }
            catch (Exception ex)
            {
                OrchestratorLogger.Log($"Unexpected error applying profile: {ex.Message}", LogLevel.Error);
                Environment.Exit(2);
            }
        }

        // ── Set volume all ────────────────────────────────────────────────────

        private static void RunSetVolumeAll(int volumePercent)
        {
            OrchestratorLogger.Log($"Setting all playback device volumes to {volumePercent}%", LogLevel.Info);
            var devices = AudioManager.GetAllDevices();
            foreach (var d in devices)
            {
                if (d.Type != "Playback" || d.State != AudioGuids.DEVICE_STATE_ACTIVE) continue;
                bool ok = AudioManager.SetVolume(d.Id, volumePercent);
                OrchestratorLogger.Log($"  '{d.FriendlyName}': {(ok ? "OK" : "FAILED")}", ok ? LogLevel.Info : LogLevel.Error);
            }
        }
    }
}
