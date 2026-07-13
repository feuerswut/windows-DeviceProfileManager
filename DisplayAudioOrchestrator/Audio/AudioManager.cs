using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

// ============================================================
// AudioManager — high-level wrapper around Core Audio COM.
// Port of AudioNative static methods from PS1 reference
// (.old/DisplayAudioOrchestrator.ps1, lines 871-1028).
// ============================================================

namespace DisplayAudioOrchestrator.Audio
{
    public sealed class AudioDeviceInfo
    {
        public string Id            { get; set; }
        public string FriendlyName  { get; set; }
        public string Type          { get; set; } // "Playback" or "Recording"
        public bool   IsDefault     { get; set; }
        public bool   IsDefaultComm { get; set; }
        public uint   State         { get; set; } // 1 = Active
        public int    VolumePercent { get; set; } // -1 if unavailable
        public bool   IsMuted       { get; set; }
    }

    public static class AudioManager
    {
        private static IMMDeviceEnumerator CreateEnumerator()
        {
            var type = Type.GetTypeFromCLSID(AudioGuids.CLSID_MMDeviceEnumerator);
            return (IMMDeviceEnumerator)Activator.CreateInstance(type);
        }

        private static string GetFriendlyName(IMMDevice device)
        {
            IPropertyStore store;
            if (device.OpenPropertyStore(AudioGuids.STGM_READ, out store) != 0) return string.Empty;
            PROPVARIANT pv;
            PROPERTYKEY key = AudioGuids.FriendlyNameKey;
            if (store.GetValue(ref key, out pv) != 0) return string.Empty;
            const ushort VT_LPWSTR = 31;
            if (pv.vt != VT_LPWSTR || pv.ptr == IntPtr.Zero) return string.Empty;
            return Marshal.PtrToStringUni(pv.ptr) ?? string.Empty;
        }

        public static List<AudioDeviceInfo> GetAllDevices()
        {
            var result    = new List<AudioDeviceInfo>();
            var enumerator = CreateEnumerator();

            string defPbId = null, defRecId = null, defPbCommId = null, defRecCommId = null;
            try { IMMDevice d; enumerator.GetDefaultAudioEndpoint(0, 0, out d); d.GetId(out defPbId);      } catch { }
            try { IMMDevice d; enumerator.GetDefaultAudioEndpoint(1, 0, out d); d.GetId(out defRecId);     } catch { }
            try { IMMDevice d; enumerator.GetDefaultAudioEndpoint(0, 2, out d); d.GetId(out defPbCommId);  } catch { }
            try { IMMDevice d; enumerator.GetDefaultAudioEndpoint(1, 2, out d); d.GetId(out defRecCommId); } catch { }

            foreach (int flow in new[] { 0, 1 }) // 0=Render/Playback, 1=Capture/Recording
            {
                IMMDeviceCollection col;
                if (enumerator.EnumAudioEndpoints(flow, AudioGuids.DEVICE_STATEMASK_ALL, out col) != 0) continue;
                uint count;
                col.GetCount(out count);
                for (uint i = 0; i < count; i++)
                {
                    IMMDevice dev;
                    if (col.Item(i, out dev) != 0) continue;
                    string id;
                    dev.GetId(out id);
                    uint state;
                    dev.GetState(out state);
                    string name = GetFriendlyName(dev);
                    int vol  = -1;
                    bool mut = false;
                    if (state == AudioGuids.DEVICE_STATE_ACTIVE)
                    {
                        try { vol = GetVolumeInternal(dev); } catch { }
                        try { mut = GetMuteInternal(dev);   } catch { }
                    }
                    result.Add(new AudioDeviceInfo
                    {
                        Id            = id,
                        FriendlyName  = name,
                        Type          = flow == 0 ? "Playback" : "Recording",
                        IsDefault     = flow == 0 ? id == defPbId  : id == defRecId,
                        IsDefaultComm = flow == 0 ? id == defPbCommId : id == defRecCommId,
                        State         = state,
                        VolumePercent = vol,
                        IsMuted       = mut
                    });
                }
            }
            return result;
        }

        public static int GetVolume(string deviceId)
        {
            var enumerator = CreateEnumerator();
            IMMDevice device;
            if (enumerator.GetDevice(deviceId, out device) != 0) return -1;
            return GetVolumeInternal(device);
        }

        public static bool SetVolume(string deviceId, int volumePercent)
        {
            var enumerator = CreateEnumerator();
            IMMDevice device;
            if (enumerator.GetDevice(deviceId, out device) != 0) return false;
            var vol  = GetVolumeInterface(device);
            if (vol == null) return false;
            Guid empty = Guid.Empty;
            vol.SetMasterVolumeLevelScalar(volumePercent / 100f, ref empty);
            Marshal.ReleaseComObject(vol);
            return true;
        }

        public static bool GetMute(string deviceId)
        {
            var enumerator = CreateEnumerator();
            IMMDevice device;
            if (enumerator.GetDevice(deviceId, out device) != 0) return false;
            return GetMuteInternal(device);
        }

        public static bool SetMute(string deviceId, bool mute)
        {
            var enumerator = CreateEnumerator();
            IMMDevice device;
            if (enumerator.GetDevice(deviceId, out device) != 0) return false;
            var vol = GetVolumeInterface(device);
            if (vol == null) return false;
            Guid empty = Guid.Empty;
            vol.SetMute(mute, ref empty);
            Marshal.ReleaseComObject(vol);
            return true;
        }

        // Sets device as default for all three COM roles (Console, Multimedia, Communications).
        public static bool SetDefaultEndpoint(string deviceId)
        {
            try
            {
                var type   = Type.GetTypeFromCLSID(AudioGuids.CLSID_PolicyConfigClient);
                var policy = (IPolicyConfig)Activator.CreateInstance(type);
                bool ok = policy.SetDefaultEndpoint(deviceId, 0) == 0;
                ok &= policy.SetDefaultEndpoint(deviceId, 1) == 0;
                ok &= policy.SetDefaultEndpoint(deviceId, 2) == 0;
                return ok;
            }
            catch { return false; }
        }

        // Finds first active device whose FriendlyName contains the pattern (case-insensitive).
        public static AudioDeviceInfo FindByPattern(string pattern, string type = null)
        {
            var all = GetAllDevices();
            foreach (var d in all)
            {
                if (d.State != AudioGuids.DEVICE_STATE_ACTIVE) continue;
                if (type != null && !d.Type.Equals(type, StringComparison.OrdinalIgnoreCase)) continue;
                if (d.FriendlyName.IndexOf(pattern, StringComparison.OrdinalIgnoreCase) >= 0) return d;
            }
            return null;
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private static IAudioEndpointVolume GetVolumeInterface(IMMDevice device)
        {
            Guid iid = AudioGuids.IID_IAudioEndpointVolume;
            IntPtr ptr;
            if (device.Activate(ref iid, AudioGuids.CLSCTX_ALL, IntPtr.Zero, out ptr) != 0) return null;
            return (IAudioEndpointVolume)Marshal.GetObjectForIUnknown(ptr);
        }

        private static int GetVolumeInternal(IMMDevice device)
        {
            var vol = GetVolumeInterface(device);
            if (vol == null) return -1;
            float level;
            vol.GetMasterVolumeLevelScalar(out level);
            Marshal.ReleaseComObject(vol);
            return (int)Math.Round(level * 100f);
        }

        private static bool GetMuteInternal(IMMDevice device)
        {
            var vol = GetVolumeInterface(device);
            if (vol == null) return false;
            bool muted;
            vol.GetMute(out muted);
            Marshal.ReleaseComObject(vol);
            return muted;
        }
    }
}
