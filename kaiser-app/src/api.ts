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

export const api = {
  getSnapshot(): Promise<SnapshotDto> {
    return invoke("get_snapshot");
  },

  listDisplays(): Promise<DisplayInfo[]> {
    return invoke("list_displays");
  },

  toggleDisplay(displayId: DisplayId): Promise<void> {
    return invoke("toggle_display", { displayId });
  },

  applyLayout(layout: Layout): Promise<void> {
    return invoke("apply_layout", { layout });
  },

  saveProfile(name: string): Promise<void> {
    return invoke("save_profile", { name });
  },

  applyProfile(name: string): Promise<void> {
    return invoke("apply_profile", { name });
  },

  deleteProfile(name: string): Promise<void> {
    return invoke("delete_profile", { name });
  },

  listProfiles(): Promise<ProfileDto[]> {
    return invoke("list_profiles");
  },

  listAudioDevices(): Promise<AudioDevice[]> {
    return invoke("list_audio_devices");
  },

  setAudioVolume(deviceId: string, volume: number): Promise<void> {
    return invoke("set_audio_volume", { deviceId, volume });
  },

  setAudioMute(deviceId: string, muted: boolean): Promise<void> {
    return invoke("set_audio_mute", { deviceId, muted });
  },

  setDefaultAudioDevice(deviceId: string): Promise<void> {
    return invoke("set_default_audio_device", { deviceId });
  },

  listDisplayModes(gdiDeviceName: string): Promise<DisplayMode[]> {
    return invoke("list_display_modes", { gdiDeviceName });
  },

  setDisplayMode(gdiDeviceName: string, mode: DisplayMode): Promise<void> {
    return invoke("set_display_mode", { gdiDeviceName, mode });
  },

  listDisplayModesForId(displayId: DisplayId): Promise<DisplayMode[]> {
    return invoke("list_display_modes_for_id", { displayId });
  },

  setDisplayModeForId(displayId: DisplayId, mode: DisplayMode): Promise<void> {
    return invoke("set_display_mode_for_id", { displayId, mode });
  },

  confirmLayout(): Promise<void> {
    return invoke("confirm_layout");
  },

  revertLayout(): Promise<void> {
    return invoke("revert_layout");
  },

  makePrimary(displayId: DisplayId): Promise<void> {
    return invoke("make_primary", { displayId });
  },

  getDisplayDpi(adapterLuid: number, targetId: number): Promise<number> {
    return invoke("get_display_dpi_cmd", { adapterLuid, targetId });
  },

  setDisplayDpi(adapterLuid: number, targetId: number, percent: number): Promise<void> {
    return invoke("set_display_dpi_cmd", { adapterLuid, targetId, percent });
  },

  updateProfile(
    name: string,
    layout: import("./types").Layout,
    dpiScales: Record<string, number>,
    audio: import("./types").AudioSetting[],
  ): Promise<void> {
    return invoke("update_profile", { name, layout, dpiScales, audio });
  },
};
