import { useState } from "react";
import { toast } from "sonner";
import { Play, Save, Trash2, RefreshCw } from "lucide-react";
import { api } from "../api";
import type { AudioDevice, ProfileDto, SnapshotDto } from "../types";

interface Props {
  snapshot: SnapshotDto;
  audioDevices: AudioDevice[];
  onRefresh: () => Promise<void>;
}

export function ProfilesTab({ snapshot, audioDevices: _audioDevices, onRefresh }: Props) {
  const [newName, setNewName] = useState("");
  const [saveBusy, setSaveBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState<string | null>(null);

  async function handleSave() {
    const name = newName.trim();
    if (!name) {
      toast.error("Profile name cannot be empty");
      return;
    }
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
        <button
          onClick={onRefresh}
          className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <RefreshCw size={12} />
          Refresh
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
        <button
          onClick={handleSave}
          disabled={saveBusy || !newName.trim()}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-700 hover:bg-blue-600 disabled:opacity-50 rounded text-sm font-medium transition-colors"
        >
          <Save size={14} />
          {saveBusy ? "Saving…" : "Save"}
        </button>
      </div>

      {/* Profile list */}
      <div className="space-y-2">
        {snapshot.profiles.length === 0 && (
          <div className="text-sm text-zinc-600 py-6 text-center">
            No profiles saved yet
          </div>
        )}
        {snapshot.profiles.map((profile) => (
          <div
            key={profile.name}
            className="rounded-lg border border-zinc-800 bg-zinc-900 p-3 flex items-start justify-between gap-3"
          >
            <div className="min-w-0">
              <div className="font-medium text-sm truncate">{profile.name}</div>
              <div className="text-xs text-zinc-500 mt-0.5 flex items-center flex-wrap gap-x-2 gap-y-0.5">
                <span>
                  {profile.layout.outputs.filter((o) => o.enabled).length} display
                  {profile.layout.outputs.filter((o) => o.enabled).length !== 1 ? "s" : ""}
                </span>
                {profile.dpi_scales && Object.keys(profile.dpi_scales).length > 0 && (
                  <span className="text-cyan-400">
                    {Object.values(profile.dpi_scales).join("/") + "% DPI"}
                  </span>
                )}
                {profile.audio.length > 0 && (
                  <span className="text-purple-400">
                    {profile.audio.map((a) => a.pattern).join(", ")}
                  </span>
                )}
              </div>
              {/* Display summary */}
              <div className="flex flex-wrap gap-1 mt-1.5">
                {profile.layout.outputs
                  .filter((o) => o.enabled)
                  .map((o, i) => (
                    <span
                      key={i}
                      className="text-xs bg-zinc-800 text-zinc-400 px-1.5 py-0.5 rounded"
                    >
                      {o.resolution.width}×{o.resolution.height}
                      {o.primary && (
                        <span className="text-blue-400 ml-1">★</span>
                      )}
                    </span>
                  ))}
              </div>
            </div>

            <div className="flex items-center gap-1 shrink-0">
              <button
                onClick={() => handleApply(profile)}
                disabled={actionBusy === profile.name}
                title="Apply profile"
                className="flex items-center gap-1 px-2.5 py-1.5 bg-green-900/40 text-green-400 hover:bg-green-900/60 border border-green-900 rounded text-xs font-medium disabled:opacity-50 transition-colors"
              >
                <Play size={12} />
                {actionBusy === profile.name ? "…" : "Apply"}
              </button>
              <button
                onClick={() => handleDelete(profile)}
                disabled={actionBusy === profile.name}
                title="Delete profile"
                className="p-1.5 text-zinc-500 hover:text-red-400 disabled:opacity-50 transition-colors rounded"
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
