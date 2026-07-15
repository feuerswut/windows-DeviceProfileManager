export interface DisplayId {
  adapter_luid: number;
  target_id: number;
  edid_hash: number | null;
}

export interface Resolution {
  width: number;
  height: number;
}

export interface Position {
  x: number;
  y: number;
}

export interface DisplayInfo {
  id: DisplayId;
  friendly_name: string;
  is_active: boolean;
  is_primary: boolean;
  resolution: Resolution;
  refresh_rate_mhz: number;
}

export interface OutputConfig {
  display_id: DisplayId;
  enabled: boolean;
  position: Position;
  resolution: Resolution;
  refresh_rate_mhz: number;
  primary: boolean;
}

export interface Layout {
  outputs: OutputConfig[];
}

export type AudioFlow = "render" | "capture";

export interface AudioDevice {
  id: string;
  name: string;
  flow: AudioFlow;
  enabled: boolean;
  volume: number;
  muted: boolean;
  is_default_console: boolean;
  is_default_comms: boolean;
}

export interface AudioSetting {
  pattern: string;
  flow: AudioFlow;
  set_default?: boolean;
  volume?: number;
  muted?: boolean;
}

export interface ProfileDto {
  name: string;
  layout: Layout;
  audio: AudioSetting[];
  /** Per-monitor DPI percentages, keyed by "adapter_luid:target_id" */
  dpi_scales?: Record<string, number>;
  /** Friendly display names captured at save time, keyed by "adapter_luid:target_id" */
  display_names?: Record<string, string>;
}

export interface SnapshotDto {
  displays: DisplayInfo[];
  layout: Layout;
  profiles: ProfileDto[];
  pending_confirmation: boolean;
  pending_confirmation_remaining_secs: number | null;
  /** GDI device names keyed as "adapter_luid:target_id" strings. */
  gdi_names: Record<string, string>;
  /** Current DPI scaling percentages keyed by "adapter_luid:target_id". */
  dpi_values: Record<string, number>;
  /** Current rotation in degrees (0/90/180/270) keyed by "adapter_luid:target_id". Absent = 0°. */
  rotation_values: Record<string, number>;
  /** Clone relationships: "luid:tid" (clone) → "luid:tid" (source). */
  clone_pairs: Record<string, string>;
}

export interface DisplayMode {
  width: number;
  height: number;
  refresh_rate_hz: number;
  bit_depth: number;
}
