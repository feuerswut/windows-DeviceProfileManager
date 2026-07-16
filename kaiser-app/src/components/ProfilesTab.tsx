import { useState, useEffect, useRef, useImperativeHandle, forwardRef } from "react";
import { toast } from "sonner";
import { Play, Save, Trash2, RefreshCw, Monitor, Mic, Volume2, ChevronDown, Edit2, X, Check, AlertTriangle } from "lucide-react";
import { api } from "../api";
import type { AudioDevice, AudioSetting, DisplayMode, Layout, OutputConfig, ProfileDto, SnapshotDto } from "../types";
import { LayoutCanvas, DPI_OPTIONS, displayKey, normalizeLayout } from "./DisplaysTab";

// ---- Helpers ----------------------------------------------------------------

function friendlyName(output: OutputConfig, snapshot: SnapshotDto): string {
  const match = snapshot.displays.find(
    (d) =>
      d.id.adapter_luid === output.display_id.adapter_luid &&
      d.id.target_id === output.display_id.target_id
  );
  return match?.friendly_name ?? `Display ${output.display_id.target_id}`;
}

function displayIndex(output: OutputConfig, snapshot: SnapshotDto): number | null {
  const idx = snapshot.displays.findIndex(
    (d) =>
      d.id.adapter_luid === output.display_id.adapter_luid &&
      d.id.target_id === output.display_id.target_id
  );
  return idx >= 0 ? idx + 1 : null;
}

// ---- Monitor card in profile summary ----------------------------------------

function MonitorCard({
  output,
  index,
  dpi,
  name,
  rotation,
  cloneSourceName,
  cloneSourceIndex,
}: {
  output: OutputConfig;
  index: number;
  dpi: number | undefined;
  name: string | undefined;
  rotation?: number;
  cloneSourceName?: string;
  cloneSourceIndex?: number;
}) {
  const hz = Math.round(output.refresh_rate_mhz / 1000);
  const displayName = name
    ? name.replace(` ${output.display_id.target_id}`, '').trim() || 'Display'
    : undefined;
  return (
    <div className={`flex items-center gap-2 rounded border px-2 py-1 text-xs ${
      output.enabled
        ? "border-zinc-700 bg-zinc-800/60 text-zinc-300"
        : "border-zinc-800 bg-zinc-900/40 text-zinc-600"
    }`}>
      <Monitor size={11} className={output.enabled ? "text-blue-400" : "text-zinc-600"} />
      <span className="font-mono text-[10px] text-zinc-500">#{index}</span>
      {displayName && <span className="text-zinc-400">{displayName}</span>}
      <span className="font-mono text-[10px] text-zinc-500">{output.display_id.target_id}</span>
      {output.enabled ? (
        cloneSourceName ? (
          <span className="flex items-center gap-1 text-green-400">
            Mirror of <Monitor size={11} />
            {cloneSourceIndex != null && <span className="font-mono text-[10px]">#{cloneSourceIndex}</span>}
            {cloneSourceName}
          </span>
        ) : (
          <>
            <span>{output.resolution.width}×{output.resolution.height}</span>
            {hz > 0 && <span className="text-zinc-500">@{hz}Hz</span>}
            {dpi != null && <span className="text-cyan-400">{dpi}%</span>}
            {rotation != null && rotation !== 0 && (
              <span className="text-violet-400 font-medium">{rotation}°</span>
            )}
          </>
        )
      ) : (
        <span className="italic">Off</span>
      )}
      {output.primary && <span className="text-yellow-400 font-bold">★</span>}
    </div>
  );
}

// ---- Audio summary chip -----------------------------------------------------

function AudioChip({ setting, devices }: { setting: AudioSetting; devices: AudioDevice[] }) {
  const dev = devices.find((d) => d.name.toLowerCase().includes(setting.pattern.toLowerCase()));
  const name = dev?.name ?? setting.pattern;
  return (
    <div className="flex items-center gap-1 rounded border border-zinc-700 bg-zinc-800/60 px-2 py-1 text-xs text-zinc-400">
      {setting.flow === "capture" ? <Mic size={11} /> : <Volume2 size={11} />}
      <span className="truncate max-w-[180px]">{name}</span>
    </div>
  );
}

// ---- Audio device section (with hidden disconnected devices) ----------------

function AudioDeviceSection({
  devices,
  isCapture,
  activePat,
  onToggle,
}: {
  devices: AudioDevice[];
  isCapture: boolean;
  activePat: string | undefined;
  onToggle: (d: AudioDevice, active: boolean) => void;
}) {
  const [showHidden, setShowHidden] = useState(false);
  const Icon = isCapture ? Mic : Volume2;
  const connected = devices.filter((d) => d.enabled);
  const disconnected = devices.filter((d) => !d.enabled);

  function renderButton(d: AudioDevice) {
    const isActive = activePat != null && d.name.toLowerCase().includes(activePat.toLowerCase());
    return (
      <button key={d.id} onClick={() => onToggle(d, !isActive)}
        className={`w-full text-left flex items-center gap-2 rounded px-2.5 py-1.5 text-xs border transition-colors ${
          isActive
            ? "border-blue-600 bg-blue-900/30 text-blue-200"
            : "border-zinc-700 bg-zinc-800/40 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
        }`}>
        <Icon size={11} />
        <span className="flex-1 truncate">{d.name}</span>
        {isActive && <Check size={11} className="shrink-0 text-blue-400" />}
      </button>
    );
  }

  return (
    <div>
      <div className="text-[10px] text-zinc-600 mb-1 flex items-center gap-1">
        <Icon size={9} /> {isCapture ? "Input" : "Output"}
      </div>
      <div className="space-y-1">
        {connected.map(renderButton)}
        {disconnected.length > 0 && (
          <div>
            <button
              onClick={() => setShowHidden((s) => !s)}
              className="flex items-center gap-1.5 text-xs text-zinc-600 hover:text-zinc-400 transition-colors py-1"
            >
              <ChevronDown size={12} className={`transition-transform ${showHidden ? "rotate-180" : ""}`} />
              {showHidden ? "Hide" : "Show"} {disconnected.length} disconnected
            </button>
            {showHidden && (
              <div className="space-y-1 mt-1 opacity-40">
                {disconnected.map(renderButton)}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ---- Inline edit panel ------------------------------------------------------

interface EditPanelProps {
  profile: ProfileDto;
  snapshot: SnapshotDto;
  audioDevices: AudioDevice[];
  onClose: () => void;
  onSaved: () => void;
}

export interface EditPanelHandle {
  save: () => void;
}

const EditPanel = forwardRef<EditPanelHandle, EditPanelProps>(
function EditPanel({ profile, snapshot, audioDevices, onClose, onSaved }, ref) {
  const [layout, setLayout] = useState<Layout>(() => JSON.parse(JSON.stringify(profile.layout)));
  const [dpiScales, setDpiScales] = useState<Record<string, number>>(
    () => profile.dpi_scales ? { ...profile.dpi_scales } : {}
  );
  const [audio, setAudio] = useState<AudioSetting[]>(() => JSON.parse(JSON.stringify(profile.audio)));
  const [displayRotations, setDisplayRotations] = useState<Record<string, number>>(
    () => profile.display_rotations ? { ...profile.display_rotations } : {}
  );
  const [cloneSources, setCloneSources] = useState<Record<string, string>>(
    () => profile.clone_sources ? { ...profile.clone_sources } : {}
  );
  const [saving, setSaving] = useState(false);
  // Pre-populate from saved_modes so offline monitors show their known resolutions immediately.
  const [modesCache, setModesCache] = useState<Record<string, DisplayMode[] | null>>(
    () => ({ ...(profile.saved_modes ?? {}) })
  );
  const [modesOpen, setModesOpen] = useState<string | null>(null);
  const [dpiOpen, setDpiOpen] = useState<string | null>(null);
  const [cloneOpen, setCloneOpen] = useState<string | null>(null);

  async function loadModes(key: string, output: OutputConfig) {
    // Always try a live query — if it succeeds, update the cache (even over saved_modes).
    // If it fails, keep whatever is already in cache (saved_modes fallback).
    setModesCache((prev) => ({ ...prev, [key]: prev[key] ?? null }));
    try {
      const modes = await api.listDisplayModesForId(output.display_id);
      modes.sort((a, b) =>
        b.width !== a.width ? b.width - a.width :
        b.height !== a.height ? b.height - a.height :
        b.refresh_rate_hz - a.refresh_rate_hz
      );
      setModesCache((prev) => ({ ...prev, [key]: modes }));
    } catch {
      // Live query failed (monitor offline) — saved_modes already in cache, keep it.
    }
  }

  function toggleEnabled(key: string) {
    setLayout((prev) => ({
      ...prev,
      outputs: prev.outputs.map((o) =>
        displayKey(o.display_id) === key ? { ...o, enabled: !o.enabled } : o
      ),
    }));
  }

  function setPrimary(key: string) {
    setLayout((prev) => ({
      ...prev,
      outputs: prev.outputs.map((o) => ({
        ...o,
        primary: displayKey(o.display_id) === key,
        enabled: displayKey(o.display_id) === key ? true : o.enabled,
      })),
    }));
  }

  function setResolution(key: string, mode: DisplayMode) {
    setLayout((prev) => ({
      ...prev,
      outputs: prev.outputs.map((o) =>
        displayKey(o.display_id) === key
          ? { ...o, resolution: { width: mode.width, height: mode.height }, refresh_rate_mhz: mode.refresh_rate_hz * 1000 }
          : o
      ),
    }));
    setModesOpen(null);
  }

  function setDpi(key: string, pct: number) {
    setDpiScales((prev) => ({ ...prev, [key]: pct }));
    setDpiOpen(null);
  }

  function setActiveAudio(device: AudioDevice, makeDefault: boolean) {
    setAudio((prev) => {
      const filtered = prev.filter(
        (a) => a.flow !== device.flow || !a.set_default
      );
      if (makeDefault) {
        return [...filtered, { pattern: device.name, flow: device.flow, set_default: true }];
      }
      return filtered;
    });
  }

  /** Ensure cloned outputs share the same position/resolution as their source. */
  function syncClonePositions(l: Layout): Layout {
    return {
      ...l,
      outputs: l.outputs.map(o => {
        const key = displayKey(o.display_id);
        const srcKey = cloneSources[key];
        if (!srcKey) return o;
        const src = l.outputs.find(s => displayKey(s.display_id) === srcKey);
        if (!src) return o;
        return { ...o, position: src.position, resolution: src.resolution };
      }),
    };
  }

  async function save() {
    setSaving(true);
    try {
      await api.updateProfile(profile.name, normalizeLayout(syncClonePositions(layout)), dpiScales, audio, displayRotations, cloneSources);
      toast.success(`Profile "${profile.name}" updated`);
      onSaved();
    } catch (err) {
      toast.error(`Save failed: ${err}`);
    } finally {
      setSaving(false);
    }
  }

  useImperativeHandle(ref, () => ({ save }));

  const renderDevices = audioDevices.filter((d) => d.flow === "render");
  const captureDevices = audioDevices.filter((d) => d.flow === "capture");

  const activeRenderPat = audio.find((a) => a.flow === "render" && a.set_default)?.pattern;
  const activeCapturePat = audio.find((a) => a.flow === "capture" && a.set_default)?.pattern;

  return (
    <div className="mt-3 rounded-lg border border-zinc-700 bg-zinc-900/50 p-4 space-y-4">
      {/* Canvas for reposition */}
      <div>
        <div className="text-xs text-zinc-500 mb-2 font-medium">Monitor Layout</div>
        <LayoutCanvas
          draft={layout}
          displays={snapshot.displays}
          rotations={displayRotations}
          clonePairs={cloneSources}
          onDraftChange={setLayout}
          height={320}
        />
      </div>

      {/* Per-monitor settings */}
      <div>
        <div className="text-xs text-zinc-500 mb-2 font-medium">Monitors</div>
        <div className="space-y-2">
          {layout.outputs.map((output, idx) => {
            const key = displayKey(output.display_id);
            const name = friendlyName(output, snapshot);
            const num = displayIndex(output, snapshot) ?? idx + 1;
            const hz = Math.round(output.refresh_rate_mhz / 1000);
            const currentDpi = dpiScales[key];
            const modes = modesCache[key];
            const active = output.enabled || output.primary;

            return (
              <div key={key} className={`rounded-lg border p-3 transition-colors ${
                active ? "border-zinc-700 bg-zinc-900" : "border-zinc-800 bg-zinc-900/50 opacity-60"
              }`}>
                <div className="flex items-start gap-3">
                  {/* Left: icon + info (mirrors DisplaysTab layout at ~80%) */}
                  <div className="flex items-start gap-2 flex-1 min-w-0">
                    <Monitor size={16} className={`mt-0.5 shrink-0 ${active ? "text-blue-400" : "text-zinc-600"}`} />
                    <div className="min-w-0">
                      <div className="font-medium text-xs flex items-center gap-1.5 flex-wrap">
                        <span className="text-[10px] text-zinc-500 font-mono">#{num}</span>
                        <span className="truncate">{name}</span>
                        <span className="text-[10px] text-zinc-500 font-mono">{output.display_id.target_id}</span>
                      </div>
                      <div className="text-[10px] text-zinc-500 mt-0.5">
                        {output.resolution.width}×{output.resolution.height} @ {hz} Hz
                      </div>
                    </div>
                  </div>

                  {/* Middle: DPI+Res (row1) / Extended+Rotation (row2) — only when active */}
                  {active && (() => {
                    const isMirror = !!cloneSources[key];
                    return (
                    <div className="flex flex-col gap-1 shrink-0">
                      {/* Row 1: DPI + Resolution — replaced by warning for mirrors */}
                      {isMirror ? (
                        <button
                          onClick={() => {/* future: show resolution compatibility popup */}}
                          className="flex items-center gap-1 text-[10px] border border-amber-600/60 bg-amber-950/40 text-amber-400 hover:bg-amber-950/70 rounded px-1.5 py-0.5 transition-colors"
                        >
                          <AlertTriangle size={10} className="shrink-0" />
                          Check valid shared resolutions!
                        </button>
                      ) : (
                      <div className="flex items-center gap-1.5">
                        {/* DPI */}
                        <div className="relative">
                          <button onClick={() => { setDpiOpen(dpiOpen === key ? null : key); setModesOpen(null); setCloneOpen(null); }}
                            className="flex items-center gap-0.5 border border-zinc-700 hover:border-zinc-500 rounded px-1.5 py-0.5 text-[10px] transition-colors text-zinc-400 hover:text-zinc-200 whitespace-nowrap">
                            {currentDpi != null ? `${currentDpi}% DPI` : "DPI"}
                            <ChevronDown size={8} className={`transition-transform ${dpiOpen === key ? "rotate-180" : ""}`} />
                          </button>
                          {dpiOpen === key && (
                            <div className="absolute left-0 top-full mt-1 z-50 w-28 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-52">
                              {DPI_OPTIONS.map((opt) => (
                                <button key={opt.value} onClick={() => setDpi(key, opt.value)}
                                  className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 ${opt.value === currentDpi ? "text-blue-400 font-medium" : "text-zinc-300"}`}>
                                  {opt.label}
                                </button>
                              ))}
                            </div>
                          )}
                        </div>
                        {/* Resolution */}
                        <div className="relative">
                          <button onClick={async () => { const next = modesOpen === key ? null : key; setModesOpen(next); setDpiOpen(null); setCloneOpen(null); if (next) await loadModes(key, output); }}
                            className="flex items-center gap-0.5 border border-zinc-700 hover:border-zinc-500 rounded px-1.5 py-0.5 text-[10px] transition-colors text-zinc-400 hover:text-zinc-200 whitespace-nowrap">
                            {output.resolution.width}×{output.resolution.height}{hz > 0 ? ` @${hz}Hz` : ""}
                            <ChevronDown size={8} className={`transition-transform ${modesOpen === key ? "rotate-180" : ""}`} />
                          </button>
                          {modesOpen === key && (
                            <div className="absolute right-0 top-full mt-1 z-50 w-44 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-56">
                              {modes == null ? <div className="px-3 py-2 text-xs text-zinc-500">Loading…</div>
                              : modes.length === 0 ? <div className="px-3 py-2 text-xs text-zinc-500">No modes</div>
                              : modes.map((m, i) => {
                                const isCurrent = m.width === output.resolution.width && m.height === output.resolution.height && m.refresh_rate_hz === hz;
                                return (
                                  <button key={i} onClick={() => setResolution(key, m)}
                                    className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 ${isCurrent ? "text-blue-400 font-medium" : "text-zinc-300"}`}>
                                    {m.width}×{m.height} @{m.refresh_rate_hz}Hz
                                  </button>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      </div>
                      )}
                      {/* Row 2: Extended picker always shown; Rotation hidden for mirrors */}
                      <div className="flex items-center gap-1.5">
                        <div className="relative">
                          {(() => {
                            const srcOut = cloneSources[key] ? layout.outputs.find(o => displayKey(o.display_id) === cloneSources[key]) : undefined;
                            const srcIdx = srcOut ? (displayIndex(srcOut, snapshot) ?? layout.outputs.indexOf(srcOut) + 1) : undefined;
                            const srcName = srcOut ? friendlyName(srcOut, snapshot) : undefined;
                            return (
                              <>
                                <button onClick={() => { setCloneOpen(cloneOpen === key ? null : key); setDpiOpen(null); setModesOpen(null); }}
                                  className={`flex items-center gap-0.5 text-[10px] border rounded px-1.5 py-0.5 transition-colors ${cloneSources[key] ? "border-green-600 text-green-300 bg-green-900/20 hover:bg-green-900/40" : "border-zinc-700 text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"}`}>
                                  {cloneSources[key] ? `Mirrors: #${srcIdx} ${srcName}` : "Extended"}
                                  <ChevronDown size={8} className={`transition-transform ${cloneOpen === key ? "rotate-180" : ""}`} />
                                </button>
                                {cloneOpen === key && (
                                  <div className="absolute left-0 top-full mt-1 z-50 w-52 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl">
                                    <button onClick={() => { setCloneSources(p => { const n = {...p}; delete n[key]; return n; }); setCloneOpen(null); }}
                                      className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 ${!cloneSources[key] ? "text-blue-400 font-medium" : "text-zinc-300"}`}>
                                      Extended (no mirror)
                                    </button>
                                    {layout.outputs.filter(o => displayKey(o.display_id) !== key && (o.enabled || o.primary)).map((o, oi) => {
                                      const oKey = displayKey(o.display_id);
                                      const oIdx = displayIndex(o, snapshot) ?? oi + 1;
                                      const oName = friendlyName(o, snapshot) || `Display ${o.display_id.target_id}`;
                                      return (
                                        <button key={oKey} onClick={() => { setCloneSources(p => ({...p, [key]: oKey})); setCloneOpen(null); }}
                                          className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 ${cloneSources[key] === oKey ? "text-green-400 font-medium" : "text-zinc-300"}`}>
                                          #{oIdx} {oName}
                                        </button>
                                      );
                                    })}
                                  </div>
                                )}
                              </>
                            );
                          })()}
                        </div>
                        {!isMirror && (
                          <div className="flex items-center gap-0.5">
                            {([0, 90, 180, 270] as const).map(deg => (
                              <button key={deg}
                                onClick={() => setDisplayRotations(p => deg === 0 ? (({ [key]: _, ...rest }) => rest)(p) : {...p, [key]: deg})}
                                className={`flex items-center justify-center w-6 h-5 rounded text-[9px] border transition-colors ${(displayRotations[key] ?? 0) === deg ? "bg-purple-900/40 border-purple-600 text-purple-300" : "border-zinc-700 text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"}`}>
                                {deg}°
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                    );
                  })()}

                  {/* Right: Primary ★ + Enabled/Disabled toggle */}
                  <div className="flex flex-col gap-1 items-end shrink-0">
                    {output.primary ? (
                      <button disabled className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium bg-yellow-900/40 text-yellow-300 border border-yellow-700 cursor-default">★ Primary</button>
                    ) : (
                      <button onClick={() => setPrimary(key)} disabled={!active} className="flex items-center gap-1 px-2 py-1 rounded text-[10px] text-zinc-400 hover:text-yellow-300 border border-zinc-700 hover:border-yellow-600 transition-colors disabled:opacity-40">☆ Primary</button>
                    )}
                    {!output.primary && (
                      <button onClick={() => toggleEnabled(key)}
                        className={`flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium border transition-colors ${
                          output.enabled
                            ? "bg-blue-900/40 text-blue-300 border-blue-800 hover:bg-red-900/40 hover:border-red-800 hover:text-red-300"
                            : "bg-zinc-800/60 text-zinc-400 border-zinc-700 hover:bg-green-900/30 hover:border-green-800 hover:text-green-300"
                        }`}>
                        {output.enabled ? "Enabled" : "Disabled"}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Audio */}
      {(renderDevices.length > 0 || captureDevices.length > 0) && (
        <div>
          <div className="text-xs text-zinc-500 mb-2 font-medium">Audio Devices</div>
          <div className="space-y-3">
            {renderDevices.length > 0 && (
              <AudioDeviceSection
                devices={renderDevices}
                isCapture={false}
                activePat={activeRenderPat}
                onToggle={setActiveAudio}
              />
            )}
            {captureDevices.length > 0 && (
              <AudioDeviceSection
                devices={captureDevices}
                isCapture={true}
                activePat={activeCapturePat}
                onToggle={setActiveAudio}
              />
            )}
          </div>
        </div>
      )}

      {/* Save / Cancel */}
      <div className="flex gap-2 pt-1">
        <button onClick={save} disabled={saving}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-700 hover:bg-blue-600 disabled:opacity-50 rounded text-xs font-medium text-white transition-colors">
          <Save size={12} />
          {saving ? "Saving…" : "Save Profile"}
        </button>
        <button onClick={onClose}
          className="px-3 py-1.5 border border-red-900 hover:border-red-700 rounded text-xs text-red-400 hover:text-red-300 transition-colors">
          Cancel
        </button>
      </div>
    </div>
  );
});

// ---- Profile card -----------------------------------------------------------

interface ProfileCardProps {
  profile: ProfileDto;
  snapshot: SnapshotDto;
  audioDevices: AudioDevice[];
  busy: boolean;
  onApply: () => void;
  onDelete: () => void;
  onRefresh: () => void;
}

function ProfileCard({ profile, snapshot, audioDevices, busy, onApply, onDelete, onRefresh }: ProfileCardProps) {
  const [editing, setEditing] = useState(false);
  const editRef = useRef<EditPanelHandle>(null);

  const renderSettings = profile.audio.filter((a) => a.flow === "render");
  const captureSettings = profile.audio.filter((a) => a.flow === "capture");

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          {/* Name */}
          <div className="font-semibold text-sm text-zinc-100 mb-2">{profile.name}</div>

          {/* Monitor sub-cards */}
          <div className="flex flex-wrap gap-1 mb-2">
            {profile.layout.outputs.map((output, idx) => {
              const num = displayIndex(output, snapshot) ?? idx + 1;
              const key = displayKey(output.display_id);
              const dpi = profile.dpi_scales?.[key];
              const name = profile.display_names?.[key];
              const rotation = profile.display_rotations?.[key];
              const cloneKey = profile.clone_sources?.[key];
              const cloneSourceOutput = cloneKey ? profile.layout.outputs.find(o => displayKey(o.display_id) === cloneKey) : undefined;
              const cloneSourceName = cloneKey
                ? (profile.display_names?.[cloneKey]?.replace(/ \d+$/, '').trim() ?? `Monitor ${cloneKey.split(':')[1]}`)
                : undefined;
              const cloneSourceIndex = cloneSourceOutput ? (displayIndex(cloneSourceOutput, snapshot) ?? profile.layout.outputs.indexOf(cloneSourceOutput) + 1) : undefined;
              return <MonitorCard key={key} output={output} index={num} dpi={dpi} name={name} rotation={rotation} cloneSourceName={cloneSourceName} cloneSourceIndex={cloneSourceIndex} />;
            })}
          </div>

          {/* Audio row */}
          {(renderSettings.length > 0 || captureSettings.length > 0) && (
            <div className="flex flex-wrap gap-1">
              {[...renderSettings, ...captureSettings].map((s, i) => (
                <AudioChip key={i} setting={s} devices={audioDevices} />
              ))}
            </div>
          )}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-1 shrink-0">
          {editing && (
            <button
              onClick={() => editRef.current?.save()}
              title="Save profile"
              className="p-1.5 rounded border border-zinc-600 text-zinc-400 bg-zinc-800 hover:bg-zinc-700 hover:text-zinc-200 transition-colors"
            >
              <Save size={13} />
            </button>
          )}
          <button onClick={() => setEditing((e) => !e)}
            title="Edit profile"
            className={`p-1.5 rounded border transition-colors ${
              editing
                ? "border-red-800 text-red-400 bg-red-900/20 hover:bg-red-900/40"
                : "border-zinc-700 text-zinc-500 hover:text-zinc-200 hover:border-zinc-500"
            }`}>
            {editing ? <X size={13} /> : <Edit2 size={13} />}
          </button>
          <button onClick={onApply} disabled={busy}
            className="flex items-center gap-1 px-2.5 py-1.5 bg-green-900/40 text-green-400 hover:bg-green-900/60 border border-green-900 rounded text-xs font-medium disabled:opacity-50 transition-colors">
            <Play size={12} />
            {busy ? "…" : "Apply"}
          </button>
          <button onClick={onDelete} disabled={busy}
            title="Delete profile"
            className="p-1.5 text-zinc-500 hover:text-red-400 disabled:opacity-50 transition-colors rounded">
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {editing && (
        <EditPanel
          ref={editRef}
          profile={profile}
          snapshot={snapshot}
          audioDevices={audioDevices}
          onClose={() => setEditing(false)}
          onSaved={() => { setEditing(false); onRefresh(); }}
        />
      )}
    </div>
  );
}

// ---- Main component ---------------------------------------------------------

interface Props {
  snapshot: SnapshotDto;
  audioDevices: AudioDevice[];
  onRefresh: () => Promise<void>;
  onApplyStart?: () => void;
  onApplyDone?: () => void;
}

export function ProfilesTab({ snapshot, audioDevices, onRefresh, onApplyStart, onApplyDone }: Props) {
  const [newName, setNewName] = useState("");
  const [saveBusy, setSaveBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ProfileDto | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  useEffect(() => {}, [snapshot]);

  async function handleSave() {
    const name = newName.trim();
    if (!name) { toast.error("Profile name cannot be empty"); return; }
    setSaveBusy(true);
    try {
      await api.saveProfile(name);
      setNewName("");
      await onRefresh();
      toast.success(`Profile "${name}" saved`);
    } catch (err) {
      toast.error(`Save failed: ${err}`);
    } finally {
      setSaveBusy(false);
    }
  }

  async function handleApply(profile: ProfileDto) {
    setActionBusy(profile.name);
    onApplyStart?.();
    try {
      await api.applyProfile(profile.name);
      await onRefresh();
      toast.success(`Applied "${profile.name}"`);
    } catch (err) {
      toast.error(`Apply failed: ${err}`);
    } finally {
      setActionBusy(null);
      onApplyDone?.();
    }
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    setDeleteBusy(true);
    setActionBusy(pendingDelete.name);
    try {
      await api.deleteProfile(pendingDelete.name);
      await onRefresh();
      toast.success(`Deleted "${pendingDelete.name}"`);
      setPendingDelete(null);
    } catch (err) {
      toast.error(`Delete failed: ${err}`);
    } finally {
      setDeleteBusy(false);
      setActionBusy(null);
    }
  }

  return (
    <div className="relative space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Saved Profiles</h2>
        <button onClick={onRefresh}
          className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors">
          <RefreshCw size={12} /> Refresh
        </button>
      </div>

      {/* Save current as new profile */}
      <div className="flex gap-2">
        <input
          type="text"
          placeholder="New profile name…"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          className="flex-1 bg-zinc-800 border border-zinc-700 rounded px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-blue-600"
        />
        <button onClick={handleSave} disabled={saveBusy || !newName.trim()}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-700 hover:bg-blue-600 disabled:opacity-50 rounded text-sm font-medium transition-colors">
          <Save size={14} />
          {saveBusy ? "Saving…" : "Save"}
        </button>
      </div>

      {/* Profile list */}
      <div className="space-y-3">
        {snapshot.profiles.length === 0 && (
          <div className="text-sm text-zinc-600 py-8 text-center">No profiles saved yet</div>
        )}
        {snapshot.profiles.map((profile) => (
          <ProfileCard
            key={profile.name}
            profile={profile}
            snapshot={snapshot}
            audioDevices={audioDevices}
            busy={actionBusy === profile.name}
            onApply={() => handleApply(profile)}
            onDelete={() => setPendingDelete(profile)}
            onRefresh={onRefresh}
          />
        ))}
      </div>

      <div className="h-[50px]" />

      {/* Delete confirmation overlay */}
      {pendingDelete && (
        <div className="absolute inset-0 z-50 pointer-events-none">
          <div className="absolute inset-0 bg-gradient-to-b from-zinc-950/80 via-zinc-950/40 to-transparent backdrop-blur-[2px] [mask-image:linear-gradient(to_bottom,black_0%,black_40%,transparent_100%)]" />
          <div className="absolute top-4 left-1/2 -translate-x-1/2 w-4/5 pointer-events-auto">
            <div className="rounded-lg border border-red-700/50 bg-red-950/40 px-5 py-3.5 flex items-center justify-between gap-5 shadow-xl backdrop-blur-sm">
              <div className="flex items-center gap-3.5 min-w-0">
                <Trash2 size={24} className="text-red-400 shrink-0" />
                <div className="font-semibold text-base text-red-200 truncate">
                  Delete profile <span className="text-red-400">{pendingDelete.name}</span>?
                </div>
              </div>
              <div className="flex gap-2.5 shrink-0">
                <button
                  onClick={confirmDelete}
                  disabled={deleteBusy}
                  className="px-4 py-1.5 rounded text-sm font-semibold bg-red-700 hover:bg-red-600 text-white transition-colors disabled:opacity-50"
                >
                  {deleteBusy ? "…" : "Delete"}
                </button>
                <button
                  onClick={() => setPendingDelete(null)}
                  disabled={deleteBusy}
                  className="px-4 py-1.5 rounded text-sm font-semibold bg-zinc-700 hover:bg-zinc-600 text-white transition-colors disabled:opacity-50"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
