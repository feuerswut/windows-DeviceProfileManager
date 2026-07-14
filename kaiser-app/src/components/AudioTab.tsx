import { useState } from "react";
import { toast } from "sonner";
import { RefreshCw, Volume2, VolumeX, Mic } from "lucide-react";
import { api } from "../api";
import type { AudioDevice } from "../types";

interface Props {
  devices: AudioDevice[];
  onRefresh: () => Promise<void>;
}

export function AudioTab({ devices, onRefresh }: Props) {
  const [pending, setPending] = useState<Set<string>>(new Set());

  function mark(id: string) {
    setPending((prev) => new Set([...prev, id]));
  }
  function unmark(id: string) {
    setPending((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }

  async function handleVolumeChange(deviceId: string, volume: number) {
    try {
      await api.setAudioVolume(deviceId, volume / 100);
    } catch (err) {
      toast.error(`Set volume failed: ${err}`);
    }
  }

  async function handleMuteToggle(device: AudioDevice) {
    mark(device.id);
    try {
      await api.setAudioMute(device.id, !device.muted);
      await onRefresh();
      toast.success(`${device.name} ${device.muted ? "unmuted" : "muted"}`);
    } catch (err) {
      toast.error(`Mute toggle failed: ${err}`);
    } finally {
      unmark(device.id);
    }
  }

  async function handleSetDefault(device: AudioDevice) {
    mark(device.id);
    try {
      await api.setDefaultAudioDevice(device.id);
      await onRefresh();
      toast.success(`${device.name} set as default`);
    } catch (err) {
      toast.error(`Set default failed: ${err}`);
    } finally {
      unmark(device.id);
    }
  }

  const renderDevices = devices.filter((d) => d.flow === "render");
  const captureDevices = devices.filter((d) => d.flow === "capture");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Audio Devices</h2>
        <button
          onClick={onRefresh}
          className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <RefreshCw size={12} />
          Refresh
        </button>
      </div>

      <DeviceGroup title="Playback" devices={renderDevices} pending={pending} isCapture={false}
        onVolumeChange={handleVolumeChange}
        onMuteToggle={handleMuteToggle}
        onSetDefault={handleSetDefault}
      />
      <DeviceGroup title="Recording" devices={captureDevices} pending={pending} isCapture={true}
        onVolumeChange={handleVolumeChange}
        onMuteToggle={handleMuteToggle}
        onSetDefault={handleSetDefault}
      />

      <div className="h-[50px]" />
    </div>
  );
}

function VolumeSlider({
  deviceId,
  initialVolume,
  enabled,
  onVolumeChange,
}: {
  deviceId: string;
  initialVolume: number;
  enabled: boolean;
  onVolumeChange: (id: string, vol: number) => Promise<void>;
}) {
  const [value, setValue] = useState(Math.round(initialVolume * 100));

  return (
    <div className="flex items-center gap-2">
      <span className="text-xs text-zinc-500 w-8 text-right">{value}%</span>
      <input
        type="range"
        min={0}
        max={100}
        value={value}
        disabled={!enabled}
        onChange={(e) => setValue(Number(e.target.value))}
        onMouseUp={(e) =>
          onVolumeChange(deviceId, Number((e.target as HTMLInputElement).value))
        }
        className="flex-1 accent-blue-500"
      />
    </div>
  );
}

function DeviceGroup({
  title,
  devices,
  pending,
  isCapture,
  onVolumeChange,
  onMuteToggle,
  onSetDefault,
}: {
  title: string;
  devices: AudioDevice[];
  pending: Set<string>;
  isCapture: boolean;
  onVolumeChange: (id: string, vol: number) => Promise<void>;
  onMuteToggle: (d: AudioDevice) => Promise<void>;
  onSetDefault: (d: AudioDevice) => Promise<void>;
}) {
  const DeviceIcon = isCapture ? Mic : Volume2;

  return (
    <div>
      <h3 className="text-xs uppercase tracking-wider text-zinc-500 mb-2">
        {title}
      </h3>
      <div className="space-y-2">
        {devices.length === 0 && (
          <div className="text-sm text-zinc-600 py-4">No devices found</div>
        )}
        {devices.map((device) => (
          <div
            key={device.id}
            className={`rounded-lg border p-3 ${
              device.is_default_console
                ? "border-blue-700 bg-blue-950/30"
                : "border-zinc-700 bg-zinc-900"
            } ${!device.enabled ? "opacity-50" : ""}`}
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2 min-w-0">
                <DeviceIcon size={14} className="text-zinc-400 shrink-0" />
                <span className="text-sm font-medium truncate max-w-[200px]">
                  {device.name}
                </span>
              </div>
              <div className="flex items-center gap-1.5 shrink-0">
                {/* Mute toggle */}
                <button
                  onClick={() => onMuteToggle(device)}
                  disabled={pending.has(device.id) || !device.enabled}
                  title={device.muted ? "Unmute" : "Mute"}
                  className={`p-1.5 rounded transition-colors ${
                    device.muted
                      ? "text-red-400 bg-red-900/30"
                      : "text-zinc-400 hover:text-zinc-200"
                  } disabled:opacity-40`}
                >
                  {device.muted ? <VolumeX size={14} /> : <Volume2 size={14} />}
                </button>

                {/* Default toggle */}
                {device.is_default_console ? (
                  <button
                    disabled
                    className="px-2.5 py-1 rounded text-xs font-semibold bg-blue-700/60 text-blue-200 border border-blue-600 cursor-default"
                    title="Current default device"
                  >
                    Active
                  </button>
                ) : (
                  <button
                    onClick={() => onSetDefault(device)}
                    disabled={pending.has(device.id) || !device.enabled}
                    title="Set as default device"
                    className="px-2.5 py-1 rounded text-xs font-semibold text-zinc-100 bg-zinc-700 hover:bg-blue-700 border border-zinc-600 hover:border-blue-500 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    {pending.has(device.id) ? "…" : "Switch"}
                  </button>
                )}
              </div>
            </div>

            {device.enabled && (
              <VolumeSlider
                deviceId={device.id}
                initialVolume={device.volume}
                enabled={device.enabled}
                onVolumeChange={onVolumeChange}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
