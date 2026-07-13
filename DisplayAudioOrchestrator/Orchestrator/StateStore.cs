using System;
using System.IO;
using Newtonsoft.Json;

// ============================================================
// StateStore — load/save config/devices.json relative to the exe directory.
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    public static class StateStore
    {
        private static readonly JsonSerializerSettings Settings = new JsonSerializerSettings
        {
            Formatting            = Formatting.Indented,
            NullValueHandling     = NullValueHandling.Ignore,
            DefaultValueHandling  = DefaultValueHandling.Ignore
        };

        public static string ConfigPath
        {
            get
            {
                string exeDir = Path.GetDirectoryName(
                    System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
                return Path.Combine(exeDir, "config", "devices.json");
            }
        }

        public static DeviceState Load()
        {
            string path = ConfigPath;
            OrchestratorLogger.Debug($"StateStore: loading from {path}");
            if (!File.Exists(path))
            {
                OrchestratorLogger.Debug("StateStore: file not found, returning empty state");
                return new DeviceState();
            }
            try
            {
                string json = File.ReadAllText(path);
                var state = JsonConvert.DeserializeObject<DeviceState>(json, Settings);
                OrchestratorLogger.Debug($"StateStore: loaded {state.Profiles.Count} profiles");
                return state ?? new DeviceState();
            }
            catch (Exception ex)
            {
                OrchestratorLogger.Log($"StateStore: failed to load devices.json: {ex.Message}", LogLevel.Error);
                return new DeviceState();
            }
        }

        public static void Save(DeviceState state)
        {
            string path = ConfigPath;
            OrchestratorLogger.Debug($"StateStore: saving to {path}");
            try
            {
                string dir = Path.GetDirectoryName(path);
                if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
                    Directory.CreateDirectory(dir);

                string json = JsonConvert.SerializeObject(state, Settings);
                File.WriteAllText(path, json);
                OrchestratorLogger.Debug("StateStore: save complete");
            }
            catch (Exception ex)
            {
                OrchestratorLogger.Log($"StateStore: failed to save devices.json: {ex.Message}", LogLevel.Error);
                throw;
            }
        }
    }
}
