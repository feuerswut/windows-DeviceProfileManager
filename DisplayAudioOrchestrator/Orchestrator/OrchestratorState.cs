using System.Collections.Generic;
using Newtonsoft.Json;

// ============================================================
// OrchestratorState — data model for devices.json.
// Profiles are stored purely by GDI short name (DISPLAY1) with
// FriendlyName kept for fallback resolution and human readability.
// ============================================================

namespace DisplayAudioOrchestrator.Orchestrator
{
    // ── Display nicknames (per-machine registry of physical displays) ─────────

    public sealed class DisplayNickname
    {
        [JsonProperty("friendlyName")]
        public string FriendlyName { get; set; }   // partial friendly name as registered

        [JsonProperty("gdiName")]
        public string GdiName      { get; set; }   // "DISPLAY1" — primary stable key

        [JsonProperty("notes")]
        public string Notes        { get; set; }
    }

    // ── Audio nicknames (per-machine registry of audio endpoints) ─────────────

    public sealed class AudioNickname
    {
        [JsonProperty("pattern")]
        public string Pattern  { get; set; }   // substring of FriendlyName to match

        [JsonProperty("type")]
        public string Type     { get; set; }   // "Playback" or "Recording"

        [JsonProperty("deviceId")]
        public string DeviceId { get; set; }   // stored for diagnostics only; not used for matching

        [JsonProperty("notes")]
        public string Notes    { get; set; }
    }

    // ── Profile display entry ─────────────────────────────────────────────────

    public sealed class ProfileDisplay
    {
        [JsonProperty("nickname")]
        public string Nickname   { get; set; }

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
        public string MirrorOf   { get; set; }   // nickname of display to mirror
    }

    // ── Profile audio entry ───────────────────────────────────────────────────

    public sealed class ProfileAudio
    {
        [JsonProperty("nickname")]
        public string Nickname   { get; set; }

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
        [JsonProperty("displays")]
        public Dictionary<string, DisplayNickname> Displays { get; set; }
            = new Dictionary<string, DisplayNickname>();

        [JsonProperty("audio")]
        public Dictionary<string, AudioNickname>   Audio    { get; set; }
            = new Dictionary<string, AudioNickname>();

        [JsonProperty("profiles")]
        public Dictionary<string, OrchestratorProfile> Profiles { get; set; }
            = new Dictionary<string, OrchestratorProfile>();
    }
}
