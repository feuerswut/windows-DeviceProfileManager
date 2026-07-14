import { useState, useRef } from "react";
import { toast } from "sonner";
import { RefreshCw, Power, Monitor, ChevronDown, Star } from "lucide-react";
import { api } from "../api";
import type { DisplayInfo, DisplayMode, Layout, OutputConfig, SnapshotDto } from "../types";

// ---- Helpers ----------------------------------------------------------------

function displayKey(id: { adapter_luid: number; target_id: number }) {
  return `${id.adapter_luid}:${id.target_id}`;
}

/** Extract Windows display number from GDI name like "\\.\DISPLAY1" → "1" */
function gdiDisplayNumber(gdiName: string | undefined): string | null {
  if (!gdiName) return null;
  const m = gdiName.match(/DISPLAY(\d+)/i);
  return m ? m[1] : null;
}

// ---- Layout Canvas ----------------------------------------------------------

const SNAP_PX = 50;

function getCanvasBounds(outputs: OutputConfig[]) {
  // Include origin and minimum 1920×1080 to keep the coordinate system stable.
  let minX = 0, minY = 0, maxX = 1920, maxY = 1080;
  for (const o of outputs) {
    minX = Math.min(minX, o.position.x);
    minY = Math.min(minY, o.position.y);
    maxX = Math.max(maxX, o.position.x + o.resolution.width);
    maxY = Math.max(maxY, o.position.y + o.resolution.height);
  }
  return { minX, minY, maxX, maxY };
}

interface LayoutCanvasProps {
  draft: Layout;
  displays: DisplayInfo[];
  onDraftChange: (l: Layout) => void;
}

function LayoutCanvas({ draft, displays, onDraftChange }: LayoutCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dragState = useRef<{
    key: string;
    startClientX: number;
    startClientY: number;
    origX: number;
    origY: number;
    frozenLayout: Layout;
    frozenBounds: ReturnType<typeof getCanvasBounds>;
  } | null>(null);

  const bounds = getCanvasBounds(draft.outputs);
  const totalW = bounds.maxX - bounds.minX;
  const totalH = bounds.maxY - bounds.minY;

  function onPointerDown(e: React.PointerEvent<HTMLDivElement>, key: string) {
    e.preventDefault();
    const output = draft.outputs.find((o) => displayKey(o.display_id) === key);
    if (!output) return;
    dragState.current = {
      key,
      startClientX: e.clientX,
      startClientY: e.clientY,
      origX: output.position.x,
      origY: output.position.y,
      frozenLayout: draft,
      frozenBounds: bounds,
    };
  }

  function onPointerMove(e: React.PointerEvent) {
    if (!dragState.current || !containerRef.current) return;
    const d = dragState.current;
    const rect = containerRef.current.getBoundingClientRect();
    const scaleX = (d.frozenBounds.maxX - d.frozenBounds.minX) / rect.width;
    const scaleY = (d.frozenBounds.maxY - d.frozenBounds.minY) / rect.height;
    const vx = (e.clientX - d.startClientX) * scaleX;
    const vy = (e.clientY - d.startClientY) * scaleY;
    const newX = Math.round((d.origX + vx) / SNAP_PX) * SNAP_PX;
    const newY = Math.round((d.origY + vy) / SNAP_PX) * SNAP_PX;

    onDraftChange({
      ...d.frozenLayout,
      outputs: d.frozenLayout.outputs.map((o) =>
        displayKey(o.display_id) === d.key
          ? { ...o, position: { x: Math.max(0, newX), y: Math.max(0, newY) } }
          : o
      ),
    });
  }

  function onPointerUp() {
    dragState.current = null;
  }

  return (
    <div
      ref={containerRef}
      className="relative w-full overflow-hidden rounded-lg border border-zinc-700 bg-zinc-950 select-none"
      style={{ aspectRatio: `${totalW}/${totalH}`, maxHeight: "200px" }}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerLeave={onPointerUp}
    >
      {draft.outputs.map((output) => {
        const key = displayKey(output.display_id);
        const display = displays.find((d) => displayKey(d.id) === key);
        const leftPct = ((output.position.x - bounds.minX) / totalW) * 100;
        const topPct = ((output.position.y - bounds.minY) / totalH) * 100;
        const widthPct = (output.resolution.width / totalW) * 100;
        const heightPct = (output.resolution.height / totalH) * 100;

        return (
          <div
            key={key}
            onPointerDown={(e) => onPointerDown(e, key)}
            className={`absolute rounded border cursor-grab active:cursor-grabbing flex flex-col items-center justify-center overflow-hidden transition-colors ${
              output.enabled
                ? output.primary
                  ? "border-yellow-500 bg-yellow-900/30 text-yellow-200"
                  : "border-blue-600 bg-blue-900/30 text-blue-300"
                : "border-zinc-600 bg-zinc-800/20 text-zinc-500 opacity-40"
            }`}
            style={{ left: `${leftPct}%`, top: `${topPct}%`, width: `${widthPct}%`, height: `${heightPct}%` }}
          >
            <span className="text-[10px] leading-tight px-1 pointer-events-none truncate w-full text-center">
              {display?.friendly_name ?? key}
            </span>
            {output.primary && (
              <span className="text-[8px] text-yellow-400 pointer-events-none">PRIMARY</span>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ---- Resolution Picker ------------------------------------------------------

interface ResolutionPickerProps {
  display: DisplayInfo;
  gdiName: string;
  onRefresh: () => Promise<void>;
}

function ResolutionPicker({ display, gdiName, onRefresh }: ResolutionPickerProps) {
  const [open, setOpen] = useState(false);
  const [modes, setModes] = useState<DisplayMode[] | null>(null);
  const [applying, setApplying] = useState(false);

  async function toggleOpen() {
    if (open) { setOpen(false); return; }
    setOpen(true);
    if (!modes) {
      try {
        const m = await api.listDisplayModes(gdiName);
        m.sort((a, b) =>
          b.width !== a.width ? b.width - a.width :
          b.height !== a.height ? b.height - a.height :
          b.refresh_rate_hz - a.refresh_rate_hz
        );
        setModes(m);
      } catch (err) {
        toast.error(`Failed to load modes: ${err}`);
        setOpen(false);
      }
    }
  }

  async function selectMode(mode: DisplayMode) {
    setApplying(true);
    setOpen(false);
    try {
      await api.setDisplayMode(gdiName, mode);
      await onRefresh();
      toast.success(`${display.friendly_name}: ${mode.width}×${mode.height} @ ${mode.refresh_rate_hz}Hz`);
    } catch (err) {
      toast.error(`Set mode failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  const currentLabel = `${display.resolution.width}×${display.resolution.height} @ ${Math.round(display.refresh_rate_mhz / 1000)}Hz`;

  return (
    <div className="relative">
      <button
        onClick={toggleOpen}
        disabled={applying}
        className="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 border border-zinc-700 hover:border-zinc-500 rounded px-2 py-1 transition-colors disabled:opacity-50 whitespace-nowrap"
      >
        {applying ? "…" : currentLabel}
        <ChevronDown size={10} className={`transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && modes && (
        <div className="absolute right-0 top-full mt-1 z-50 w-52 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-64">
          {modes.map((mode, i) => {
            const isActive =
              mode.width === display.resolution.width &&
              mode.height === display.resolution.height &&
              mode.refresh_rate_hz === Math.round(display.refresh_rate_mhz / 1000);
            return (
              <button
                key={i}
                onClick={() => selectMode(mode)}
                className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 transition-colors ${
                  isActive ? "text-blue-400 font-medium" : "text-zinc-300"
                }`}
              >
                {mode.width}×{mode.height} @ {mode.refresh_rate_hz}Hz
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ---- Main component ---------------------------------------------------------

interface Props {
  snapshot: SnapshotDto;
  onRefresh: () => Promise<void>;
}

export function DisplaysTab({ snapshot, onRefresh }: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  const [draftLayout, setDraftLayout] = useState<Layout | null>(null);
  const [applyingLayout, setApplyingLayout] = useState(false);
  const [confirmBusy, setConfirmBusy] = useState(false);

  const { displays, layout, pending_confirmation, pending_confirmation_remaining_secs, gdi_names } = snapshot;
  const activeCount = displays.filter((d) => d.is_active).length;
  const currentLayout = draftLayout ?? layout;
  const isDirty = draftLayout !== null;

  async function toggleDisplay(display: DisplayInfo) {
    if (pending_confirmation) {
      toast.error("Confirm or revert the current layout change first.");
      return;
    }
    const key = displayKey(display.id);
    setBusy(key);
    try {
      await api.toggleDisplay(display.id);
      setDraftLayout(null);
      await onRefresh();
      toast.success(`${display.friendly_name} ${display.is_active ? "disabled" : "enabled"}`);
    } catch (err) {
      toast.error(`Toggle failed: ${err}`);
    } finally {
      setBusy(null);
    }
  }

  async function confirmLayout() {
    setConfirmBusy(true);
    try {
      await api.confirmLayout();
      await onRefresh();
      toast.success("Layout confirmed");
    } catch (err) {
      toast.error(`Confirm failed: ${err}`);
    } finally {
      setConfirmBusy(false);
    }
  }

  async function revertLayout() {
    setConfirmBusy(true);
    try {
      await api.revertLayout();
      await onRefresh();
      toast.success("Layout reverted");
    } catch (err) {
      toast.error(`Revert failed: ${err}`);
    } finally {
      setConfirmBusy(false);
    }
  }

  async function applyDraft() {
    if (!draftLayout) return;
    setApplyingLayout(true);
    try {
      await api.applyLayout(draftLayout);
      setDraftLayout(null);
      await onRefresh();
      toast.success("Layout applied");
    } catch (err) {
      toast.error(`Apply failed: ${err}`);
    } finally {
      setApplyingLayout(false);
    }
  }

  async function makePrimary(display: DisplayInfo) {
    if (pending_confirmation) {
      toast.error("Confirm or revert the current layout change first.");
      return;
    }
    const key = displayKey(display.id);
    setBusy(key);
    try {
      await api.makePrimary(display.id);
      await onRefresh();
      toast.success(`${display.friendly_name} set as primary`);
    } catch (err) {
      toast.error(`Make primary failed: ${err}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="space-y-4">
      {/* Pending confirmation banner */}
      {pending_confirmation && (
        <div className="rounded-lg border border-yellow-600 bg-yellow-950/40 px-4 py-3 text-sm">
          <div className="flex items-center justify-between mb-2">
            <span className="text-yellow-300 font-medium">
              Layout change — confirm to keep or revert within{" "}
              {pending_confirmation_remaining_secs != null
                ? `${Math.ceil(pending_confirmation_remaining_secs)}s`
                : "…"}
            </span>
          </div>
          <div className="flex gap-2">
            <button
              onClick={confirmLayout}
              disabled={confirmBusy}
              className="px-3 py-1.5 rounded text-xs font-medium bg-green-700 hover:bg-green-600 text-white transition-colors disabled:opacity-50"
            >
              {confirmBusy ? "…" : "Confirm"}
            </button>
            <button
              onClick={revertLayout}
              disabled={confirmBusy}
              className="px-3 py-1.5 rounded text-xs font-medium bg-red-800 hover:bg-red-700 text-white transition-colors disabled:opacity-50"
            >
              {confirmBusy ? "…" : "Revert"}
            </button>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Display Layout</h2>
        <button
          onClick={onRefresh}
          className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          <RefreshCw size={12} />
          Refresh
        </button>
      </div>

      <LayoutCanvas
        draft={currentLayout}
        displays={displays}
        onDraftChange={setDraftLayout}
      />

      <div className="flex items-center gap-2 min-h-[28px]">
        {isDirty ? (
          <>
            <button
              onClick={applyDraft}
              disabled={applyingLayout || pending_confirmation}
              className="px-3 py-1.5 rounded text-xs font-medium bg-blue-700 hover:bg-blue-600 text-white transition-colors disabled:opacity-50"
            >
              {applyingLayout ? "Applying…" : "Apply Layout"}
            </button>
            <button
              onClick={() => setDraftLayout(null)}
              className="px-3 py-1.5 rounded text-xs text-zinc-400 hover:text-zinc-200 border border-zinc-700 hover:border-zinc-500 transition-colors"
            >
              Reset
            </button>
            <span className="text-xs text-zinc-500">Drag to reposition</span>
          </>
        ) : (
          <p className="text-xs text-zinc-600">Drag monitors above to reposition, then click Apply Layout.</p>
        )}
      </div>

      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Connected Displays</h2>
      </div>

      <div className="grid gap-3">
        {displays.map((display) => {
          const key = displayKey(display.id);
          const output = layout.outputs.find(
            (o) =>
              o.display_id.adapter_luid === display.id.adapter_luid &&
              o.display_id.target_id === display.id.target_id
          );
          const gdiName = gdi_names?.[key];
          const dispNum = gdiDisplayNumber(gdiName);

          return (
            <div
              key={key}
              className={`rounded-lg border p-4 transition-colors ${
                display.is_active
                  ? "border-zinc-700 bg-zinc-900"
                  : "border-zinc-800 bg-zinc-900/50 opacity-60"
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex items-center gap-3">
                  <Monitor
                    size={20}
                    className={display.is_active ? "text-blue-400" : "text-zinc-600"}
                  />
                  <div>
                    <div className="font-medium text-sm flex items-center gap-2">
                      {dispNum && (
                        <span className="text-xs text-zinc-500 font-mono">#{dispNum}</span>
                      )}
                      {display.friendly_name}
                      {display.is_primary && (
                        <span className="text-xs text-yellow-400 flex items-center gap-0.5">
                          <Star size={10} fill="currentColor" /> Primary
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-zinc-500 mt-0.5">
                      {display.resolution.width}×{display.resolution.height} @{" "}
                      {Math.round(display.refresh_rate_mhz / 1000)} Hz
                    </div>
                    {output && display.is_active && (
                      <div className="text-xs text-zinc-600 mt-0.5">
                        Position: ({output.position.x}, {output.position.y})
                      </div>
                    )}
                  </div>
                </div>

                <div className="flex items-center gap-2 flex-shrink-0">
                  {display.is_active && gdiName && (
                    <ResolutionPicker
                      display={display}
                      gdiName={gdiName}
                      onRefresh={onRefresh}
                    />
                  )}
                  {display.is_active && !display.is_primary && (
                    <button
                      onClick={() => makePrimary(display)}
                      disabled={busy === key || pending_confirmation}
                      title="Set as primary display"
                      className="flex items-center gap-1 px-2 py-1.5 rounded text-xs text-zinc-400 hover:text-yellow-300 border border-zinc-700 hover:border-yellow-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <Star size={11} />
                      Primary
                    </button>
                  )}
                  <button
                    onClick={() => toggleDisplay(display)}
                    disabled={busy === key || (display.is_active && activeCount <= 1)}
                    title={
                      display.is_active && activeCount <= 1
                        ? "Cannot disable the last active display"
                        : undefined
                    }
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
                      display.is_active
                        ? "bg-red-900/40 text-red-400 hover:bg-red-900/60 border border-red-900"
                        : "bg-green-900/40 text-green-400 hover:bg-green-900/60 border border-green-900"
                    } disabled:opacity-50 disabled:cursor-not-allowed`}
                  >
                    <Power size={12} />
                    {busy === key ? "…" : display.is_active ? "Disable" : "Enable"}
                  </button>
                </div>
              </div>
            </div>
          );
        })}

        {displays.length === 0 && (
          <div className="text-center text-zinc-500 py-12 text-sm">
            No displays detected
          </div>
        )}
      </div>
    </div>
  );
}
