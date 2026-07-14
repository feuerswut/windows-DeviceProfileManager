import { useState, useRef, useEffect } from "react";
import { toast } from "sonner";
import { RefreshCw, Power, Monitor, ChevronDown } from "lucide-react";
import { api } from "../api";
import type { DisplayId, DisplayInfo, DisplayMode, Layout, OutputConfig, SnapshotDto } from "../types";

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

const DPI_OPTIONS = [
  { label: "100%", value: 100 },
  { label: "125%", value: 125 },
  { label: "150%", value: 150 },
  { label: "175%", value: 175 },
  { label: "200%", value: 200 },
  { label: "225%", value: 225 },
  { label: "250%", value: 250 },
  { label: "300%", value: 300 },
];

// ---- Layout Canvas (Monarch-style) ------------------------------------------

function getLayoutBounds(outputs: OutputConfig[]) {
  if (outputs.length === 0) return { left: 0, top: 0, right: 1920, bottom: 1080 };
  const left = Math.min(...outputs.map((o) => o.position.x));
  const top = Math.min(...outputs.map((o) => o.position.y));
  const right = Math.max(...outputs.map((o) => o.position.x + o.resolution.width));
  const bottom = Math.max(...outputs.map((o) => o.position.y + o.resolution.height));
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

const SNAP_PX = 50;

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
    frozenBounds: ReturnType<typeof getLayoutBounds>;
    scale: number;
  } | null>(null);

  const activeOutputs = draft.outputs.filter((o) => o.enabled);
  const bounds = getLayoutBounds(activeOutputs.length > 0 ? activeOutputs : draft.outputs);
  const totalW = bounds.right - bounds.left;
  const totalH = bounds.bottom - bounds.top;
  const scale = Math.min(700 / Math.max(totalW, 1), 280 / Math.max(totalH, 1));
  const canvasW = Math.max(340, totalW * scale + 24);
  const canvasH = Math.max(160, totalH * scale + 24);

  const monitorNumbers = new Map(displays.map((d, i) => [displayKey(d.id), i + 1]));

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
      scale,
    };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: React.PointerEvent) {
    if (!dragState.current) return;
    const d = dragState.current;
    const vx = (e.clientX - d.startClientX) / d.scale;
    const vy = (e.clientY - d.startClientY) / d.scale;
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
    <div className="overflow-auto rounded-lg border border-zinc-700 p-3">
      <div
        ref={containerRef}
        className="relative rounded-md border border-zinc-800 bg-zinc-900/40 select-none"
        style={{ width: `${canvasW}px`, height: `${canvasH}px`, minHeight: "140px" }}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
      >
        {draft.outputs.map((output) => {
          const key = displayKey(output.display_id);
          const display = displays.find((d) => displayKey(d.id) === key);
          const monNum = monitorNumbers.get(key);
          const active = output.enabled;
          const left = (output.position.x - bounds.left) * scale + 12;
          const top = (output.position.y - bounds.top) * scale + 12;
          const w = Math.max(42, output.resolution.width * scale);
          const h = Math.max(28, output.resolution.height * scale);

          return (
            <div
              key={key}
              onPointerDown={(e) => active ? onPointerDown(e, key) : undefined}
              className={`absolute flex min-w-0 flex-col justify-between rounded-md border p-1.5 text-[10px] ${
                active
                  ? output.primary
                    ? "border-yellow-500/50 bg-yellow-900/20 text-yellow-200 cursor-grab active:cursor-grabbing"
                    : "border-blue-500/30 bg-blue-900/15 text-blue-200 cursor-grab active:cursor-grabbing"
                  : "border-zinc-600 bg-zinc-800/30 text-zinc-500"
              }`}
              style={{ left: `${left}px`, top: `${top}px`, width: `${w}px`, height: `${h}px` }}
            >
              <div className="flex min-w-0 items-start justify-between gap-1">
                <div className="flex min-w-0 items-start gap-1">
                  {monNum != null && (
                    <span className="inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded border border-current/40 bg-zinc-900/60 px-1 text-[9px] font-bold leading-none">
                      {monNum}
                    </span>
                  )}
                  <span className="truncate font-medium leading-tight pointer-events-none">
                    {display?.friendly_name ?? key}
                  </span>
                </div>
                {output.primary && (
                  <span className="shrink-0 rounded-full border border-yellow-500/50 px-1 py-px text-[7px] font-semibold uppercase tracking-wide text-yellow-400 pointer-events-none">
                    Primary
                  </span>
                )}
              </div>
              <span className="text-[9px] leading-none text-zinc-500 pointer-events-none">
                {active ? `${output.resolution.width}×${output.resolution.height}` : "Detached"}
              </span>
            </div>
          );
        })}
      </div>
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
    setModes(null);
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

// ---- DPI Picker (per-monitor) -----------------------------------------------

interface DpiPickerProps {
  displayId: DisplayId;
}

function DpiPicker({ displayId }: DpiPickerProps) {
  const [open, setOpen] = useState(false);
  const [currentDpi, setCurrentDpi] = useState<number | null>(null);
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    api.getDisplayDpi(displayId.adapter_luid, displayId.target_id)
      .then(setCurrentDpi)
      .catch(() => {});
  }, [displayId.adapter_luid, displayId.target_id]);

  async function selectDpi(percent: number) {
    if (percent === currentDpi) { setOpen(false); return; }
    setApplying(true);
    setOpen(false);
    try {
      await api.setDisplayDpi(displayId.adapter_luid, displayId.target_id, percent);
      setCurrentDpi(percent);
      toast.success(`DPI set to ${percent}%`);
    } catch (err) {
      toast.error(`Set DPI failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  const label = currentDpi != null ? `${currentDpi}% DPI` : "DPI…";

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        disabled={applying || currentDpi == null}
        className="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 border border-zinc-700 hover:border-zinc-500 rounded px-2 py-1 transition-colors disabled:opacity-50 whitespace-nowrap"
      >
        {applying ? "…" : label}
        <ChevronDown size={10} className={`transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-1 z-50 w-32 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-64">
          {DPI_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => selectDpi(opt.value)}
              className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 transition-colors ${
                opt.value === currentDpi ? "text-blue-400 font-medium" : "text-zinc-300"
              }`}
            >
              {opt.label}
            </button>
          ))}
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
    <div className="relative space-y-4">
      {/* Pending confirmation — floating banner just below the tab bar */}
      {pending_confirmation && (
        <div className="sticky top-0 z-40 rounded-lg border border-yellow-500 bg-yellow-950/90 backdrop-blur px-4 py-3 text-sm shadow-lg">
          <div className="flex items-center justify-between gap-4">
            <span className="text-yellow-300 font-medium">
              Confirm layout change within{" "}
              {pending_confirmation_remaining_secs != null
                ? `${Math.ceil(pending_confirmation_remaining_secs)}s`
                : "…"}{" "}
              or it will revert automatically
            </span>
            <div className="flex gap-2 shrink-0">
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
        {displays.map((display, index) => {
          const key = displayKey(display.id);
          const output = layout.outputs.find(
            (o) =>
              o.display_id.adapter_luid === display.id.adapter_luid &&
              o.display_id.target_id === display.id.target_id
          );
          const gdiName = gdi_names?.[key];
          const dispNum = gdiDisplayNumber(gdiName) ?? String(index + 1);
          const isBusy = busy === key;

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
                      <span className="text-xs text-zinc-500 font-mono">#{dispNum}</span>
                      {display.friendly_name}
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

                <div className="flex items-center gap-2 flex-shrink-0 flex-wrap justify-end">
                  {/* DPI picker — per-monitor, fetches current DPI on mount */}
                  {display.is_active && (
                    <DpiPicker displayId={display.id} />
                  )}

                  {/* Resolution picker */}
                  {display.is_active && gdiName && (
                    <ResolutionPicker
                      display={display}
                      gdiName={gdiName}
                      onRefresh={onRefresh}
                    />
                  )}

                  {/* Primary button — all active monitors, toggle style */}
                  {display.is_active && (
                    display.is_primary ? (
                      <button
                        disabled
                        className="flex items-center gap-1 px-2 py-1.5 rounded text-xs font-medium bg-yellow-900/40 text-yellow-300 border border-yellow-700 cursor-default opacity-90"
                        title="This is the primary display"
                      >
                        ★ Primary
                      </button>
                    ) : (
                      <button
                        onClick={() => makePrimary(display)}
                        disabled={isBusy || pending_confirmation}
                        title="Set as primary display"
                        className="flex items-center gap-1 px-2 py-1.5 rounded text-xs text-zinc-400 hover:text-yellow-300 border border-zinc-700 hover:border-yellow-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        ☆ Primary
                      </button>
                    )
                  )}

                  {/* Enable/Disable button */}
                  <button
                    onClick={() => toggleDisplay(display)}
                    disabled={isBusy || (display.is_active && activeCount <= 1)}
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
                    {isBusy ? "…" : display.is_active ? "Disable" : "Enable"}
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
