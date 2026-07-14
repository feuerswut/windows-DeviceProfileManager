import { useEffect, useState, useCallback } from "react";
import { Toaster, toast } from "sonner";
import { Monitor, Volume2, Layers } from "lucide-react";
import { api } from "./api";
import type { SnapshotDto, AudioDevice } from "./types";
import { DisplaysTab } from "./components/DisplaysTab";
import { AudioTab } from "./components/AudioTab";
import { ProfilesTab } from "./components/ProfilesTab";

type Tab = "displays" | "audio" | "profiles";

export default function App() {
  const [tab, setTab] = useState<Tab>("displays");
  const [snapshot, setSnapshot] = useState<SnapshotDto | null>(null);
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);

  const refreshSnapshot = useCallback(async () => {
    try {
      const s = await api.getSnapshot();
      setSnapshot(s);
    } catch (err) {
      toast.error(`Failed to refresh: ${err}`);
    }
  }, []);

  const refreshAudio = useCallback(async () => {
    try {
      const devices = await api.listAudioDevices();
      setAudioDevices(devices);
    } catch (err) {
      toast.error(`Failed to list audio devices: ${err}`);
    }
  }, []);

  useEffect(() => {
    Promise.all([refreshSnapshot(), refreshAudio()]).finally(() =>
      setLoading(false)
    );
  }, [refreshSnapshot, refreshAudio]);

  // Poll every 3s: snapshot drives auto-rollback + display state; audio detects external changes
  useEffect(() => {
    if (loading) return;
    const id = setInterval(() => {
      refreshSnapshot();
      refreshAudio();
    }, 3000);
    return () => clearInterval(id);
  }, [loading, refreshSnapshot, refreshAudio]);

  const tabs: { id: Tab; label: string; icon: React.ReactNode }[] = [
    { id: "displays", label: "Displays", icon: <Monitor size={16} /> },
    { id: "audio", label: "Audio", icon: <Volume2 size={16} /> },
    { id: "profiles", label: "Profiles", icon: <Layers size={16} /> },
  ];

  return (
    <div className="flex flex-col h-screen bg-zinc-950 text-zinc-100">
      <Toaster position="bottom-right" theme="dark" richColors />

      {/* Header */}
      <header className="flex items-center gap-2 px-4 py-3 border-b border-zinc-800">
        <span className="font-semibold text-white tracking-wide">Kaiser</span>
        <span className="text-xs text-zinc-500 ml-auto">
          Display &amp; Audio Manager
        </span>
      </header>

      {/* Tab bar */}
      <nav className="flex border-b border-zinc-800">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
              tab === t.id
                ? "border-b-2 border-blue-500 text-white"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </nav>

      {/* Content */}
      <main className="flex-1 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full text-zinc-500">
            Loading…
          </div>
        ) : (
          <>
            {tab === "displays" && snapshot && (
              <DisplaysTab
                snapshot={snapshot}
                onRefresh={refreshSnapshot}
              />
            )}
            {tab === "audio" && (
              <AudioTab
                devices={audioDevices}
                onRefresh={refreshAudio}
              />
            )}
            {tab === "profiles" && snapshot && (
              <ProfilesTab
                snapshot={snapshot}
                audioDevices={audioDevices}
                onRefresh={refreshSnapshot}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}
