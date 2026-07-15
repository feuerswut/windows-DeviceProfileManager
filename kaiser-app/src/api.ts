import { invoke } from "@tauri-apps/api/core";
import type {
  AudioDevice,
  DisplayId,
  DisplayInfo,
  DisplayMode,
  Layout,
  ProfileDto,
  SnapshotDto,
} from "./types";

async function inv<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    const msg = `${cmd}: ${err}`;
    console.error("[kaiser]", msg);
    invoke("frontend_log", { level: "error", message: msg }).catch(() => {});
    throw err;
  }
}

export const api = {
  getSnapshot(): Promise<SnapshotDto> {
    return inv("get_snapshot");
  },

  listDisplays(): Promise<DisplayInfo[]> {
    return inv("list_displays");
  },

  toggleDisplay(displayId: DisplayId): Promise<void> {
    return inv("toggle_display", { displayId });
  },

  applyLayout(layout: Layout): Promise<void> {
    return inv("apply_layout", { layout });
  },

  saveProfile(name: string): Promise<void> {
    return inv("save_profile", { name });
  },

  applyProfile(name: string): Promise<void> {
    return inv("apply_profile", { name });
  },

  deleteProfile(name: string): Promise<void> {
    return inv("delete_profile", { name });
  },

  listProfiles(): Promise<ProfileDto[]> {
    return inv("list_profiles");
  },

  listAudioDevices(): Promise<AudioDevice[]> {
    return inv("list_audio_devices");
  },

  setAudioVolume(deviceId: string, volume: number): Promise<void> {
    return inv("set_audio_volume", { deviceId, volume });
  },

  setAudioMute(deviceId: string, muted: boolean): Promise<void> {
    return inv("set_audio_mute", { deviceId, muted });
  },

  setDefaultAudioDevice(deviceId: string): Promise<void> {
    return inv("set_default_audio_device", { deviceId });
  },

  listDisplayModes(gdiDeviceName: string): Promise<DisplayMode[]> {
    return inv("list_display_modes", { gdiDeviceName });
  },

  setDisplayMode(gdiDeviceName: string, mode: DisplayMode): Promise<void> {
    return inv("set_display_mode", { gdiDeviceName, mode });
  },

  listDisplayModesForId(displayId: DisplayId): Promise<DisplayMode[]> {
    return inv("list_display_modes_for_id", { displayId });
  },

  setDisplayModeForId(displayId: DisplayId, mode: DisplayMode): Promise<void> {
    return inv("set_display_mode_for_id", { displayId, mode });
  },

  confirmLayout(): Promise<void> {
    return inv("confirm_layout");
  },

  revertLayout(): Promise<void> {
    return inv("revert_layout");
  },

  makePrimary(displayId: DisplayId): Promise<void> {
    return inv("make_primary", { displayId });
  },

  setDisplayDpi(adapterLuid: number, targetId: number, percent: number): Promise<void> {
    return inv("set_display_dpi_cmd", { adapterLuid, targetId, percent });
  },

  updateProfile(
    name: string,
    layout: import("./types").Layout,
    dpiScales: Record<string, number>,
    audio: import("./types").AudioSetting[],
    displayRotations: Record<string, number>,
    cloneSources: Record<string, string>,
  ): Promise<void> {
    return inv("update_profile", { name, layout, dpiScales, audio, displayRotations, cloneSources });
  },

  /** Set rotation for a single active display (0, 90, 180, 270 degrees). */
  setDisplayRotation(adapterLuid: number, targetId: number, degrees: number): Promise<void> {
    return inv("set_display_rotation", { adapterLuid, targetId, degrees });
  },

  /** Set clone source for a display. Pass srcAdapterLuid=0, srcTargetId=0 to remove cloning. */
  setCloneSource(
    cloneAdapterLuid: number,
    cloneTargetId: number,
    srcAdapterLuid: number,
    srcTargetId: number,
  ): Promise<void> {
    return inv("set_clone_source", {
      cloneAdapterLuid, cloneTargetId, srcAdapterLuid, srcTargetId,
    });
  },

  /** Invalidate the backend display snapshot, forcing a fresh Windows query. */
  refreshBackend(): Promise<void> {
    return inv("refresh_backend");
  },
};
