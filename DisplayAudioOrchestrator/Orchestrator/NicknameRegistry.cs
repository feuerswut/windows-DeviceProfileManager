using System;
using System.Collections.Generic;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;

// ============================================================
// NicknameRegistry — map human nicknames to live display/audio devices.
// Resolution order for displays:
//   1. GdiName exact match (DISPLAY1 == DISPLAY1)  — primary stable key
//   2. FriendlyName substring match                — fallback
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    public static class NicknameRegistry
    {
        // ── Display ───────────────────────────────────────────────────────────

        public static void RegisterDisplay(DeviceState state, string nickname,
            string friendlyName, string gdiName)
        {
            state.Displays[nickname] = new DisplayNickname
            {
                FriendlyName = friendlyName,
                GdiName      = gdiName
            };
            OrchestratorLogger.Log($"Registered display nick '{nickname}' → GDI:{gdiName} ({friendlyName})", LogLevel.Info);
        }

        // Returns the live DisplayInfo entry matching the registered nickname.
        // Returns null if not found — caller decides how to treat unresolved nicknames.
        public static DisplayInfo ResolveDisplay(DeviceState state, string nickname,
            List<DisplayInfo> liveDisplays)
        {
            if (!state.Displays.TryGetValue(nickname, out var reg))
            {
                OrchestratorLogger.Log($"NicknameRegistry: no registration for display nick '{nickname}'", LogLevel.Warn);
                return null;
            }

            // 1. GDI exact match
            if (!string.IsNullOrEmpty(reg.GdiName))
            {
                foreach (var d in liveDisplays)
                    if (d.GdiShortName.Equals(reg.GdiName, StringComparison.OrdinalIgnoreCase))
                    {
                        OrchestratorLogger.Debug($"  Resolved '{nickname}' → {d.GdiShortName} via GDI name");
                        return d;
                    }
                OrchestratorLogger.Debug($"  GDI name '{reg.GdiName}' not found live; trying FriendlyName fallback");
            }

            // 2. FriendlyName substring fallback
            if (!string.IsNullOrEmpty(reg.FriendlyName))
            {
                foreach (var d in liveDisplays)
                    if (d.FriendlyName != null &&
                        d.FriendlyName.IndexOf(reg.FriendlyName, StringComparison.OrdinalIgnoreCase) >= 0)
                    {
                        OrchestratorLogger.Debug($"  Resolved '{nickname}' → {d.GdiShortName} via FriendlyName fallback");
                        return d;
                    }
            }

            OrchestratorLogger.Log($"NicknameRegistry: could not resolve display nick '{nickname}' " +
                $"(registered GDI={reg.GdiName}, friendly={reg.FriendlyName})", LogLevel.Warn);
            return null;
        }

        public static void RemoveDisplay(DeviceState state, string nickname)
        {
            if (state.Displays.Remove(nickname))
                OrchestratorLogger.Log($"Removed display nick '{nickname}'", LogLevel.Info);
        }

        // ── Audio ─────────────────────────────────────────────────────────────

        public static void RegisterAudio(DeviceState state, string nickname,
            string pattern, string type, string deviceId = null)
        {
            state.Audio[nickname] = new AudioNickname
            {
                Pattern  = pattern,
                Type     = type,
                DeviceId = deviceId
            };
            OrchestratorLogger.Log($"Registered audio nick '{nickname}' → pattern:'{pattern}' type:{type}", LogLevel.Info);
        }

        // Returns the first active audio device matching the registered pattern+type.
        public static AudioDeviceInfo ResolveAudio(DeviceState state, string nickname,
            List<AudioDeviceInfo> liveDevices)
        {
            if (!state.Audio.TryGetValue(nickname, out var reg))
            {
                OrchestratorLogger.Log($"NicknameRegistry: no registration for audio nick '{nickname}'", LogLevel.Warn);
                return null;
            }

            foreach (var d in liveDevices)
            {
                if (d.State != Audio.AudioGuids.DEVICE_STATE_ACTIVE) continue;
                if (!string.IsNullOrEmpty(reg.Type) &&
                    !d.Type.Equals(reg.Type, StringComparison.OrdinalIgnoreCase)) continue;
                if (d.FriendlyName.IndexOf(reg.Pattern, StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    OrchestratorLogger.Debug($"  Resolved audio '{nickname}' → '{d.FriendlyName}'");
                    return d;
                }
            }

            OrchestratorLogger.Log($"NicknameRegistry: could not resolve audio nick '{nickname}' " +
                $"(pattern='{reg.Pattern}', type={reg.Type})", LogLevel.Warn);
            return null;
        }

        public static void RemoveAudio(DeviceState state, string nickname)
        {
            if (state.Audio.Remove(nickname))
                OrchestratorLogger.Log($"Removed audio nick '{nickname}'", LogLevel.Info);
        }
    }
}
