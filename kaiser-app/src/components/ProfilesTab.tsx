import { useState, useEffect, useRef, useImperativeHandle, forwardRef } from "react";
import { toast } from "sonner";
import { Play, Save, Trash2, RefreshCw, Monitor, Mic, Volume2, ChevronDown, Edit2, X, Check } from "lucide-react";
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
}: {
  output: OutputConfig;
  index: number;
  dpi: number | undefined;
}) {
  const hz = Math.round(output.refresh_rate_mhz / 1000);
  return (
    <div
      className={`flex items-center gap-2 rounded border px-2 py-1 text-xs ${
        output.enabled
          ? "border-zinc-700 bg-zinc-800/60 text-zinc-300"
          : "border-zinc-800 bg-zinc-900/40 text-zinc-600"
      }`}
    >
      <Monitor size={11} className={output.enabled ? "text-blue-400" : "text-zinc-600"} />
      <span className="font-mono text-[10px] text-zinc-500">#{index}</span>
      {output.enabled ? (
        <>
          <span>{output.resolution.width}×{output.resolution.height}</span>
          {hz > 0 && <span className="text-zinc-500">@{hz}Hz</span>}
          {dpi != null && <span className="text-cyan-400">{dpi}%</span>}
          {output.primary && <span className="text-yellow-400 font-bold">★</span>}
        </>
      ) : (
        <span className="italic">Off</span>
      )}
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
  const [saving, setSaving] = useState(false);
  const [modesCache, setModesCache] = useState<Record<string, DisplayMode[] | null>>({});
  const [modesOpen, setModesOpen] = useState<string | null>(null);
  const [dpiOpen, setDpiOpen] = useState<string | null>(null);

  async function loadModes(key: string, output: OutputConfig) {
    if (modesCache[key] !== undefined) return;
    setModesCache((prev) => ({ ...prev, [key]: null }));
    try {
      const modes = await api.listDisplayModesForId(output.display_id);
      modes.sort((a, b) =>
        b.width !== a.width ? b.width - a.width :
        b.height !== a.height ? b.height - a.height :
        b.refresh_rate_hz - a.refresh_rate_hz
      );
      setModesCache((prev) => ({ ...prev, [key]: modes }));
    } catch {
      setModesCache((prev) => ({ ...prev, [key]: [] }));
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

  async function save() {
    setSaving(true);
    try {
      await api.updateProfile(profile.name, normalizeLayout(layout), dpiScales, audio);
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
          onDraftChange={setLayout}
          height={200}
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

            return (
              <div key={key} className={`flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-xs ${
                output.enabled ? "border-zinc-700 bg-zinc-800/40" : "border-zinc-800 bg-zinc-900/30 opacity-60"
              }`}>
                <Monitor size={12} className={output.enabled ? "text-blue-400" : "text-zinc-600"} />
                <span className="font-mono text-zinc-500">#{num}</span>
                <span className="font-medium text-zinc-300 flex-1 truncate">{name}</span>

                {/* Active toggle */}
                <button
                  onClick={() => toggleEnabled(key)}
                  className={`px-2 py-1 rounded text-[10px] font-medium border transition-colors ${
                    output.enabled
                      ? "bg-blue-700/50 border-blue-600 text-blue-200 hover:bg-red-900/50 hover:border-red-700 hover:text-red-300"
                      : "bg-zinc-800 border-zinc-700 text-zinc-400 hover:bg-green-900/40 hover:border-green-700 hover:text-green-300"
                  }`}
                >
                  {output.enabled ? "Enabled" : "Disabled"}
                </button>

                {output.enabled && (
                  <>
                    {/* DPI */}
                    <div className="relative">
                      <button
                        onClick={() => {
                          setDpiOpen(dpiOpen === key ? null : key);
                          setModesOpen(null);
                        }}
                        className="flex items-center gap-1 border border-zinc-700 hover:border-zinc-500 rounded px-2 py-1 transition-colors text-zinc-400 hover:text-zinc-200 whitespace-nowrap"
                      >
                        {currentDpi != null ? `${currentDpi}% DPI` : "DPI"}
                        <ChevronDown size={9} className={`transition-transform ${dpiOpen === key ? "rotate-180" : ""}`} />
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
                      <button
                        onClick={async () => {
                          const next = modesOpen === key ? null : key;
                          setModesOpen(next);
                          setDpiOpen(null);
                          if (next) await loadModes(key, output);
                        }}
                        className="flex items-center gap-1 border border-zinc-700 hover:border-zinc-500 rounded px-2 py-1 transition-colors text-zinc-400 hover:text-zinc-200 whitespace-nowrap"
                      >
                        {output.resolution.width}×{output.resolution.height}{hz > 0 ? ` @${hz}Hz` : ""}
                        <ChevronDown size={9} className={`transition-transform ${modesOpen === key ? "rotate-180" : ""}`} />
                      </button>
                      {modesOpen === key && (
                        <div className="absolute right-0 top-full mt-1 z-50 w-44 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-56">
                          {modes == null ? (
                            <div className="px-3 py-2 text-xs text-zinc-500">Loading…</div>
                          ) : modes.length === 0 ? (
                            <div className="px-3 py-2 text-xs text-zinc-500">No modes</div>
                          ) : (
                            modes.map((m, i) => {
                              const active =
                                m.width === output.resolution.width &&
                                m.height === output.resolution.height &&
                                m.refresh_rate_hz === hz;
                              return (
                                <button key={i} onClick={() => setResolution(key, m)}
                                  className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 ${active ? "text-blue-400 font-medium" : "text-zinc-300"}`}>
                                  {m.width}×{m.height} @{m.refresh_rate_hz}Hz
                                </button>
                              );
                            })
                          )}
                        </div>
                      )}
                    </div>
                  </>
                )}
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
              <div>
                <div className="text-[10px] text-zinc-600 mb-1 flex items-center gap-1">
                  <Volume2 size={9} /> Output
                </div>
                <div className="space-y-1">
                  {renderDevices.map((d) => {
                    const isActive = activeRenderPat != null && d.name.toLowerCase().includes(activeRenderPat.toLowerCase());
                    return (
                      <button key={d.id} onClick={() => setActiveAudio(d, !isActive)}
                        className={`w-full text-left flex items-center gap-2 rounded px-2.5 py-1.5 text-xs border transition-colors ${
                          isActive
                            ? "border-blue-600 bg-blue-900/30 text-blue-200"
                            : "border-zinc-700 bg-zinc-800/40 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
                        }`}>
                        <Volume2 size={11} />
                        <span className="flex-1 truncate">{d.name}</span>
                        {isActive && <Check size={11} className="shrink-0 text-blue-400" />}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            {captureDevices.length > 0 && (
              <div>
                <div className="text-[10px] text-zinc-600 mb-1 flex items-center gap-1">
                  <Mic size={9} /> Input
                </div>
                <div className="space-y-1">
                  {captureDevices.map((d) => {
                    const isActive = activeCapturePat != null && d.name.toLowerCase().includes(activeCapturePat.toLowerCase());
                    return (
                      <button key={d.id} onClick={() => setActiveAudio(d, !isActive)}
                        className={`w-full text-left flex items-center gap-2 rounded px-2.5 py-1.5 text-xs border transition-colors ${
                          isActive
                            ? "border-blue-600 bg-blue-900/30 text-blue-200"
                            : "border-zinc-700 bg-zinc-800/40 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
                        }`}>
                        <Mic size={11} />
                        <span className="flex-1 truncate">{d.name}</span>
                        {isActive && <Check size={11} className="shrink-0 text-blue-400" />}
                      </button>
                    );
                  })}
                </div>
              </div>
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
          className="px-3 py-1.5 border border-zinc-700 hover:border-zinc-500 rounded text-xs text-zinc-400 hover:text-zinc-200 transition-colors">
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
              return <MonitorCard key={key} output={output} index={num} dpi={dpi} />;
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
              className="p-1.5 rounded border border-green-700 text-green-400 bg-green-900/30 hover:bg-green-900/50 transition-colors"
            >
              <Save size={13} />
            </button>
          )}
          <button onClick={() => setEditing((e) => !e)}
            title="Edit profile"
            className={`p-1.5 rounded border transition-colors ${
              editing
                ? "border-blue-600 text-blue-400 bg-blue-900/30"
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
}

export function ProfilesTab({ snapshot, audioDevices, onRefresh }: Props) {
  const [newName, setNewName] = useState("");
  const [saveBusy, setSaveBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState<string | null>(null);

  // Keep profiles fresh in the background (snapshot already polled every 3s in App.tsx)
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
    try {
      await api.applyProfile(profile.name);
      await onRefresh();
      toast.success(`Applied "${profile.name}"`);
    } catch (err) {
      toast.error(`Apply failed: ${err}`);
    } finally {
      setActionBusy(null);
    }
  }

  async function handleDelete(profile: ProfileDto) {
    if (!confirm(`Delete profile "${profile.name}"?`)) return;
    setActionBusy(profile.name);
    try {
      await api.deleteProfile(profile.name);
      await onRefresh();
      toast.success(`Deleted "${profile.name}"`);
    } catch (err) {
      toast.error(`Delete failed: ${err}`);
    } finally {
      setActionBusy(null);
    }
  }

  return (
    <div className="space-y-5">
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
            onDelete={() => handleDelete(profile)}
            onRefresh={onRefresh}
          />
        ))}
      </div>

      <div className="h-[50px]" />
    </div>
  );
}
