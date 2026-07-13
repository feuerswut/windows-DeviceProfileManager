using System.Collections.Generic;
using Newtonsoft.Json;

// ============================================================
// OrchestratorState — data model for devices.json.
// Displays are referenced by GDI short name (DISPLAY1) directly.
// Audio devices are referenced by FriendlyName pattern + type directly.
// No nickname indirection layer.
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    // ── Profile display entry ─────────────────────────────────────────────────

    public sealed class ProfileDisplay
    {
        [JsonProperty("gdiName")]
        public string GdiName    { get; set; }   // "DISPLAY1" — primary Windows display number

        [JsonProperty("active")]
        public bool?  Active     { get; set; }

        [JsonProperty("primary")]
        public bool?  Primary    { get; set; }

        [JsonProperty("width")]
        public int?   Width      { get; set; }

        [JsonProperty("height")]
        public int?   Height     { get; set; }

        [JsonProperty("hz")]
        public int?   Hz         { get; set; }

        [JsonProperty("dpiPercent")]
        public int?   DpiPercent { get; set; }

        [JsonProperty("hdr")]
        public bool?  Hdr        { get; set; }

        [JsonProperty("rotation")]
        public int?   Rotation   { get; set; }

        [JsonProperty("mirrorOf")]
        public string MirrorOf   { get; set; }   // GDI name of display to mirror
    }

    // ── Profile audio entry ───────────────────────────────────────────────────

    public sealed class ProfileAudio
    {
        [JsonProperty("pattern")]
        public string Pattern    { get; set; }   // substring of FriendlyName to match

        [JsonProperty("type")]
        public string Type       { get; set; }   // "Playback" or "Recording"

        [JsonProperty("setDefault")]
        public bool?  SetDefault { get; set; }

        [JsonProperty("volume")]
        public int?   Volume     { get; set; }   // 0-100

        [JsonProperty("mute")]
        public bool?  Mute       { get; set; }
    }

    // ── Process to launch after profile is applied ────────────────────────────

    public sealed class StartProcess
    {
        [JsonProperty("path")]
        public string Path    { get; set; }

        [JsonProperty("args")]
        public string Args    { get; set; }

        [JsonProperty("asAdmin")]
        public bool   AsAdmin { get; set; }
    }

    // ── Full profile ──────────────────────────────────────────────────────────

    public sealed class OrchestratorProfile
    {
        [JsonProperty("displays")]
        public List<ProfileDisplay> Displays       { get; set; } = new List<ProfileDisplay>();

        [JsonProperty("audio")]
        public List<ProfileAudio>   Audio          { get; set; } = new List<ProfileAudio>();

        [JsonProperty("startProcesses")]
        public List<StartProcess>   StartProcesses { get; set; } = new List<StartProcess>();
    }

    // ── Root device state (persisted to config/devices.json) ─────────────────

    public sealed class DeviceState
    {
        [JsonProperty("profiles")]
        public Dictionary<string, OrchestratorProfile> Profiles { get; set; }
            = new Dictionary<string, OrchestratorProfile>();
    }
}
