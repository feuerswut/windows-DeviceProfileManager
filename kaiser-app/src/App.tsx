import { useEffect, useState, useCallback, useRef } from "react";
import { Toaster, toast } from "sonner";
import { Monitor, Volume2, Layers } from "lucide-react";
import { api } from "./api";
import type { SnapshotDto, AudioDevice } from "./types";
import { DisplaysTab } from "./components/DisplaysTab";
import { AudioTab } from "./components/AudioTab";
import { ProfilesTab } from "./components/ProfilesTab";

function ConfirmBanner({
  remainingSecs,
  onConfirm,
  onRevert,
}: {
  remainingSecs: number | null;
  onConfirm: () => Promise<void>;
  onRevert: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  // Local countdown — initialized from backend, ticks every 100ms, re-syncs on each backend poll
  const [display, setDisplay] = useState<number | null>(remainingSecs);
  const localRef = useRef<number | null>(remainingSecs);

  // Re-sync whenever the backend gives us a fresh value (every ~3s)
  useEffect(() => {
    if (remainingSecs != null) {
      localRef.current = remainingSecs;
      setDisplay(remainingSecs);
    }
  }, [remainingSecs]);

  // Tick every 100ms for a smooth countdown
  useEffect(() => {
    const id = setInterval(() => {
      if (localRef.current == null) return;
      localRef.current = Math.max(0, localRef.current - 0.1);
      setDisplay(localRef.current);
    }, 100);
    return () => clearInterval(id);
  }, []);

  async function act(fn: () => Promise<void>) {
    setBusy(true);
    try { await fn(); } finally { setBusy(false); }
  }

  const secs = display != null ? Math.ceil(display) : null;

  return (
    <div className="rounded-lg border border-yellow-600/50 bg-yellow-950/40 px-5 py-3.5 flex items-center justify-between gap-5 shadow-xl backdrop-blur-sm">
      <div className="flex items-center gap-3.5 min-w-0">
        <Monitor size={24} className="text-yellow-400 shrink-0" />
        <div className="font-semibold text-base text-yellow-200">Layout change pending</div>
        {secs != null && (
          <span className="text-sm tabular-nums text-yellow-400">
            reverts in {secs}s
          </span>
        )}
      </div>
      <div className="flex gap-2.5 shrink-0">
        <button
          onClick={() => act(onConfirm)}
          disabled={busy}
          className="px-4 py-1.5 rounded text-sm font-semibold bg-green-700 hover:bg-green-600 text-white transition-colors disabled:opacity-50"
        >
          {busy ? "…" : "Confirm"}
        </button>
        <button
          onClick={() => act(onRevert)}
          disabled={busy}
          className="px-4 py-1.5 rounded text-sm font-semibold bg-red-900 hover:bg-red-800 text-white transition-colors disabled:opacity-50"
        >
          {busy ? "…" : "Revert"}
        </button>
      </div>
    </div>
  );
}

function ApplyingBanner() {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setElapsed((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const message = elapsed >= 5 ? "Still trying to apply profile…" : "Applying profile… please wait";

  return (
    <div className="rounded-lg border border-blue-600/50 bg-blue-950/40 px-5 py-3.5 flex items-center gap-3.5 shadow-xl backdrop-blur-sm">
      <Monitor size={24} className="text-blue-400 shrink-0 animate-pulse" />
      <div className="font-semibold text-base text-blue-200">{message}</div>
    </div>
  );
}

type Tab = "displays" | "audio" | "profiles";

export default function App() {
  const [tab, setTab] = useState<Tab>("displays");
  const [snapshot, setSnapshot] = useState<SnapshotDto | null>(null);
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);

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

  async function handleConfirm() {
    try {
      await api.confirmLayout();
      await refreshSnapshot();
      toast.success("Layout confirmed");
    } catch (err) {
      toast.error(`Confirm failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  async function handleRevert() {
    try {
      await api.revertLayout();
      await refreshSnapshot();
      toast.success("Layout reverted");
    } catch (err) {
      toast.error(`Revert failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  // Poll every 3s: snapshot drives auto-rollback + display state; audio detects external changes
  useEffect(() => {
    if (loading) return;
    const id = setInterval(() => {
      refreshSnapshot();
      refreshAudio();
    }, 3000);
    return () => clearInterval(id);
  }, [loading, refreshSnapshot, refreshAudio]);

  function switchTab(next: Tab) {
    if (next === tab) return;
    setTab(next);
    // Refresh whichever data source the new tab needs
    if (next === "audio") {
      refreshAudio();
    } else {
      refreshSnapshot();
    }
  }

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
            onClick={() => switchTab(t.id)}
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
      <div className="relative flex-1 overflow-hidden">
        {!loading && (applying || snapshot?.pending_confirmation) && (
          <div className="absolute inset-0 z-50 pointer-events-none">
            {/* Blur gradient scrim — fades from top, leaves bottom readable */}
            <div className="absolute inset-0 bg-gradient-to-b from-zinc-950/80 via-zinc-950/40 to-transparent backdrop-blur-[2px] [mask-image:linear-gradient(to_bottom,black_0%,black_40%,transparent_100%)]" />
            {/* Card centred at 80% width, pinned near the top */}
            <div className="absolute top-4 left-1/2 -translate-x-1/2 w-4/5 pointer-events-auto">
              {snapshot?.pending_confirmation ? (
                <ConfirmBanner
                  remainingSecs={snapshot.pending_confirmation_remaining_secs}
                  onConfirm={handleConfirm}
                  onRevert={handleRevert}
                />
              ) : (
                <ApplyingBanner />
              )}
            </div>
          </div>
        )}
      <main className="h-full overflow-auto p-4 scrollbar-thin">
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
                onApplyStart={() => setApplying(true)}
                onApplyDone={() => setApplying(false)}
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
                onApplyStart={() => setApplying(true)}
                onApplyDone={() => setApplying(false)}
              />
            )}
          </>
        )}
      </main>
      </div>
    </div>
  );
}
