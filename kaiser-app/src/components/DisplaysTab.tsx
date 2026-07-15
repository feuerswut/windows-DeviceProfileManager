import { useState, useRef, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { RefreshCw, Power, Monitor, ChevronDown, RotateCw, Copy, AlertTriangle } from "lucide-react";
import { api } from "../api";
import type { DisplayId, DisplayInfo, DisplayMode, Layout, OutputConfig, SnapshotDto } from "../types";

// ---- Helpers ----------------------------------------------------------------

export function displayKey(id: { adapter_luid: number; target_id: number }) {
  return `${id.adapter_luid}:${id.target_id}`;
}

function gdiDisplayNumber(gdiName: string | undefined): string | null {
  if (!gdiName) return null;
  const m = gdiName.match(/DISPLAY(\d+)/i);
  return m ? m[1] : null;
}

export const DPI_OPTIONS = [
  { label: "100%", value: 100 },
  { label: "125%", value: 125 },
  { label: "150%", value: 150 },
  { label: "175%", value: 175 },
  { label: "200%", value: 200 },
  { label: "225%", value: 225 },
  { label: "250%", value: 250 },
  { label: "300%", value: 300 },
];

const SNAP_THRESHOLD = 60; // virtual pixels — how close before an edge snaps
const CANVAS_SIZE = 6000;
const ROTATION_OPTIONS = [0, 90, 180, 270] as const;

// ---- Helpers ----------------------------------------------------------------

/** Returns the visual (effective) dimensions accounting for 90°/270° rotation. */
function effectiveSize(o: OutputConfig, rotationDeg: number): { width: number; height: number } {
  if (rotationDeg === 90 || rotationDeg === 270) return { width: o.resolution.height, height: o.resolution.width };
  return { width: o.resolution.width, height: o.resolution.height };
}

function getBounds(outputs: OutputConfig[], rotations: Record<string, number>) {
  if (outputs.length === 0) return { left: 0, top: 0, right: 1920, bottom: 1080, width: 1920, height: 1080 };
  const left = Math.min(...outputs.map((o) => o.position.x));
  const top = Math.min(...outputs.map((o) => o.position.y));
  const right = Math.max(...outputs.map((o) => o.position.x + effectiveSize(o, rotations[displayKey(o.display_id)] ?? 0).width));
  const bottom = Math.max(...outputs.map((o) => o.position.y + effectiveSize(o, rotations[displayKey(o.display_id)] ?? 0).height));
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

/**
 * Snap `(rawX, rawY)` of the dragged monitor (with size w×h) to the nearest
 * edge of any other enabled monitor. Axes are independent — we pick the
 * closest snap candidate per axis.
 */
function edgeSnap(
  rawX: number, rawY: number, w: number, h: number,
  others: OutputConfig[], rotations: Record<string, number>
): { x: number; y: number } {
  let bestX = rawX, bestY = rawY;
  let dxBest = SNAP_THRESHOLD + 1, dyBest = SNAP_THRESHOLD + 1;

  for (const o of others) {
    if (!o.enabled) continue;
    const { width: ow, height: oh } = effectiveSize(o, rotations[displayKey(o.display_id)] ?? 0);
    const ol = o.position.x, ot = o.position.y;
    const or_ = ol + ow, ob = ot + oh;

    // --- X candidates ---
    // our left → their right (place us to the right of them)
    const dx1 = Math.abs(rawX - or_);
    if (dx1 < dxBest) { dxBest = dx1; bestX = or_; }
    // our right → their left (place us to the left of them)
    const dx2 = Math.abs(rawX + w - ol);
    if (dx2 < dxBest) { dxBest = dx2; bestX = ol - w; }
    // align left edges
    const dx3 = Math.abs(rawX - ol);
    if (dx3 < dxBest) { dxBest = dx3; bestX = ol; }
    // align right edges
    const dx4 = Math.abs(rawX + w - or_);
    if (dx4 < dxBest) { dxBest = dx4; bestX = or_ - w; }

    // --- Y candidates ---
    // our top → their bottom (place us below them)
    const dy1 = Math.abs(rawY - ob);
    if (dy1 < dyBest) { dyBest = dy1; bestY = ob; }
    // our bottom → their top (place us above them)
    const dy2 = Math.abs(rawY + h - ot);
    if (dy2 < dyBest) { dyBest = dy2; bestY = ot - h; }
    // align top edges
    const dy3 = Math.abs(rawY - ot);
    if (dy3 < dyBest) { dyBest = dy3; bestY = ot; }
    // align bottom edges
    const dy4 = Math.abs(rawY + h - ob);
    if (dy4 < dyBest) { dyBest = dy4; bestY = ob - h; }
  }

  return { x: Math.round(bestX), y: Math.round(bestY) };
}

/**
 * Normalize layout so the primary monitor sits at (0,0).
 * This is what Windows requires. Other monitors stay relative to primary.
 * If no primary, normalize to the bounding box origin instead.
 */
export function normalizeLayout(layout: Layout): Layout {
  const primary = layout.outputs.find((o) => o.primary && o.enabled);
  const ref = primary ?? layout.outputs.filter((o) => o.enabled)[0];
  if (!ref) return layout;
  const dx = ref.position.x;
  const dy = ref.position.y;
  if (dx === 0 && dy === 0) return layout;
  return {
    ...layout,
    outputs: layout.outputs.map((o) => ({
      ...o,
      position: { x: o.position.x - dx, y: o.position.y - dy },
    })),
  };
}

interface CanvasProps {
  draft: Layout;
  displays: DisplayInfo[];
  rotations: Record<string, number>;
  clonePairs: Record<string, string>;
  onDraftChange: (l: Layout) => void;
  height?: number;
}

export function LayoutCanvas({ draft, displays, rotations, clonePairs, onDraftChange, height = 512 }: CanvasProps) {
  const outerRef = useRef<HTMLDivElement>(null);
  const [outerSize, setOuterSize] = useState({ w: 700, h: height });
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [snapAnimating, setSnapAnimating] = useState(false);
  const dragRef = useRef<
    | { type: "canvas"; sx: number; sy: number; ox: number; oy: number }
    | { type: "monitor"; key: string; sx: number; sy: number; ovx: number; ovy: number; frozenLayout: Layout; scale: number }
    | null
  >(null);

  // Measure outer container
  useEffect(() => {
    const el = outerRef.current;
    if (!el) return;
    const obs = new ResizeObserver((entries) => {
      for (const e of entries) {
        setOuterSize({ w: e.contentRect.width, h: e.contentRect.height });
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  const allOutputs = draft.outputs;
  const activeOutputs = allOutputs.filter((o) => o.enabled);

  // Clone relationships: which keys are clones, and which sources are mirrored by whom
  const cloneKeySet = new Set(Object.keys(clonePairs));
  const mirroredBy: Record<string, string[]> = {};
  for (const [cloneKey, srcKey] of Object.entries(clonePairs)) {
    (mirroredBy[srcKey] ??= []).push(cloneKey);
  }
  // Only render non-clone outputs — clones are shown as a badge on their source
  const visibleOutputs = activeOutputs.filter((o) => !cloneKeySet.has(displayKey(o.display_id)));

  const bounds = getBounds(visibleOutputs.length > 0 ? visibleOutputs : allOutputs, rotations);

  // Scale: fit bounding box with 30% margin (15% each side)
  const scale = Math.min(
    (outerSize.w * 0.7) / Math.max(bounds.width, 1),
    (outerSize.h * 0.7) / Math.max(bounds.height, 1),
    1.0 // don't zoom in beyond 1px:1px
  );

  const computeCenter = useCallback(() => {
    const bboxCx = (bounds.left + bounds.right) / 2;
    const bboxCy = (bounds.top + bounds.bottom) / 2;
    return {
      x: outerSize.w / 2 - bboxCx * scale,
      y: outerSize.h / 2 - bboxCy * scale,
    };
  }, [bounds.left, bounds.right, bounds.top, bounds.bottom, outerSize.w, outerSize.h, scale]);

  // Auto-center when layout changes externally
  const layoutSigRef = useRef("");
  useEffect(() => {
    const sig = visibleOutputs.map((o) => `${o.display_id.adapter_luid}:${o.display_id.target_id}:${o.position.x}:${o.position.y}:${o.resolution.width}:${o.resolution.height}:${rotations[displayKey(o.display_id)] ?? 0}`).join("|");
    if (sig !== layoutSigRef.current) {
      layoutSigRef.current = sig;
      setOffset(computeCenter());
    }
  }, [draft, computeCenter]);

  // Re-center when the container is first measured (ResizeObserver fires after initial render
  // with the real size, but the layoutSig effect already ran with the default 700px width)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { setOffset(computeCenter()); }, [outerSize.w, outerSize.h]);

  // Snap back if bounding box is 70%+ offscreen — animates 50% toward center
  function checkRecenter(newOffset: { x: number; y: number }) {
    const bbl = newOffset.x + bounds.left * scale;
    const bbt = newOffset.y + bounds.top * scale;
    const bbr = newOffset.x + bounds.right * scale;
    const bbb = newOffset.y + bounds.bottom * scale;
    const visW = Math.max(0, Math.min(bbr, outerSize.w) - Math.max(bbl, 0));
    const visH = Math.max(0, Math.min(bbb, outerSize.h) - Math.max(bbt, 0));
    const visArea = visW * visH;
    const totalArea = bounds.width * scale * bounds.height * scale;
    if (totalArea > 0 && visArea / totalArea < 0.75) {
      const center = computeCenter();
      setSnapAnimating(true);
      setOffset({
        x: newOffset.x + (center.x - newOffset.x) * 0.5,
        y: newOffset.y + (center.y - newOffset.y) * 0.5,
      });
    }
  }

  // Canvas pan (background)
  function onCanvasPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    setSnapAnimating(false);
    dragRef.current = { type: "canvas", sx: e.clientX, sy: e.clientY, ox: offset.x, oy: offset.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  // Monitor drag
  function onMonitorPointerDown(e: React.PointerEvent<HTMLDivElement>, key: string) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const output = draft.outputs.find((o) => displayKey(o.display_id) === key);
    if (!output) return;
    dragRef.current = {
      type: "monitor",
      key,
      sx: e.clientX,
      sy: e.clientY,
      ovx: output.position.x,
      ovy: output.position.y,
      frozenLayout: draft,
      scale,
    };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: React.PointerEvent) {
    const d = dragRef.current;
    if (!d) return;
    if (d.type === "canvas") {
      const newOffset = { x: d.ox + e.clientX - d.sx, y: d.oy + e.clientY - d.sy };
      setOffset(newOffset);
    } else {
      const dvx = (e.clientX - d.sx) / d.scale;
      const dvy = (e.clientY - d.sy) / d.scale;
      const rawX = d.ovx + dvx;
      const rawY = d.ovy + dvy;

      const dragged = d.frozenLayout.outputs.find((o) => displayKey(o.display_id) === d.key);
      const dragRot = rotations[d.key] ?? 0;
      const { width: w, height: h } = dragged ? effectiveSize(dragged, dragRot) : { width: 1920, height: 1080 };
      const others = d.frozenLayout.outputs.filter(
        (o) => displayKey(o.display_id) !== d.key
      );

      const { x, y } = edgeSnap(rawX, rawY, w, h, others, rotations);
      onDraftChange({
        ...d.frozenLayout,
        outputs: d.frozenLayout.outputs.map((o) =>
          displayKey(o.display_id) === d.key ? { ...o, position: { x, y } } : o
        ),
      });
    }
  }

  function onPointerUp(e: React.PointerEvent) {
    const d = dragRef.current;
    dragRef.current = null;
    if (d?.type === "canvas") {
      const newOffset = { x: d.ox + e.clientX - d.sx, y: d.oy + e.clientY - d.sy };
      checkRecenter(newOffset);
    }
  }

  const monitorNumbers = new Map(displays.map((d, i) => [displayKey(d.id), i + 1]));

  return (
    <div
      ref={outerRef}
      className="relative rounded-lg border border-zinc-700 bg-zinc-900/20 overflow-hidden cursor-grab active:cursor-grabbing"
      style={{ height: `${height}px` }}
      onPointerDown={onCanvasPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerLeave={onPointerUp}
    >
      {/* Infinite canvas */}
      <div
        style={{
          position: "absolute",
          width: `${CANVAS_SIZE}px`,
          height: `${CANVAS_SIZE}px`,
          transform: `translate(${offset.x}px, ${offset.y}px)`,
          transition: snapAnimating ? "transform 320ms cubic-bezier(0.22, 0.61, 0.36, 1)" : undefined,
          touchAction: "none",
        }}
        onTransitionEnd={() => setSnapAnimating(false)}
      >
        {visibleOutputs.map((output) => {
          const key = displayKey(output.display_id);
          const display = displays.find((d) => displayKey(d.id) === key);
          const monNum = monitorNumbers.get(key);
          const active = output.enabled;
          const rot = rotations[key] ?? 0;
          const mirroredByList = (mirroredBy[key] ?? []).map(ck => ({
            num: monitorNumbers.get(ck),
            name: displays.find(d => displayKey(d.id) === ck)?.friendly_name,
          }));
          const { width: ew, height: eh } = effectiveSize(output, rot);
          const x = output.position.x * scale;
          const y = output.position.y * scale;
          const w = Math.max(42, ew * scale);
          const h = Math.max(28, eh * scale);

          return (
            <div
              key={key}
              onPointerDown={(e) => onMonitorPointerDown(e, key)}
              style={{ position: "absolute", left: `${x}px`, top: `${y}px`, width: `${w}px`, height: `${h}px` }}
              className={`flex flex-col justify-between rounded-md border p-1.5 text-[10px] select-none cursor-grab active:cursor-grabbing ${
                active
                  ? output.primary
                    ? "border-yellow-500/60 bg-yellow-900/25 text-yellow-200"
                    : "border-blue-500/40 bg-blue-900/20 text-blue-100"
                  : "border-zinc-600/60 bg-zinc-800/40 text-zinc-500"
              }`}
            >
              <div className="flex min-w-0 items-start justify-between gap-1 pointer-events-none">
                <div className="flex min-w-0 items-start gap-1">
                  {monNum != null && (
                    <span className="inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded border border-current/40 bg-zinc-900/70 px-1 text-[9px] font-bold leading-none">
                      {monNum}
                    </span>
                  )}
                  <span className="truncate font-medium leading-tight">{display?.friendly_name ?? key}</span>
                </div>
                <div className="flex shrink-0 items-center gap-0.5">
                  {output.primary && <span className="rounded-full border border-yellow-500/50 px-1 py-px text-[7px] font-semibold uppercase text-yellow-400">Primary</span>}
                </div>
              </div>
              <span className="text-[9px] leading-none text-zinc-500 pointer-events-none">
                {active ? `${ew}×${eh}${rot ? ` ${rot}°` : ""}` : "Detached"}
              </span>
              {mirroredByList.map((m, i) => (
                <span key={i} className="text-[8px] leading-tight text-green-400 pointer-events-none flex items-center gap-0.5">
                  <Monitor size={7} /> {m.num != null ? `#${m.num}` : ""} {m.name ?? ""}
                </span>
              ))}
            </div>
          );
        })}
      </div>

      {/* Hint */}
      <div className="absolute bottom-2 right-3 text-[10px] text-zinc-600 pointer-events-none select-none">
        Drag monitors · Scroll canvas
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
        <div className="absolute right-0 top-full mt-1 z-50 w-44 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl overflow-y-auto max-h-64">
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
                {mode.width}×{mode.height} @{mode.refresh_rate_hz}Hz
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
  currentDpi: number | undefined;
  onRefresh: () => Promise<void>;
}

function DpiPicker({ displayId, currentDpi, onRefresh }: DpiPickerProps) {
  const [open, setOpen] = useState(false);
  const [applying, setApplying] = useState(false);

  async function selectDpi(percent: number) {
    if (percent === currentDpi) { setOpen(false); return; }
    setApplying(true);
    setOpen(false);
    try {
      await api.setDisplayDpi(displayId.adapter_luid, displayId.target_id, percent);
      await onRefresh();
      toast.success(`DPI set to ${percent}%`);
    } catch (err) {
      toast.error(`Set DPI failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        disabled={applying}
        className="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 border border-zinc-700 hover:border-zinc-500 rounded px-2 py-1 transition-colors disabled:opacity-50 whitespace-nowrap"
      >
        {applying ? "…" : currentDpi != null ? `${currentDpi}% DPI` : "DPI"}
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

// ---- Rotation Picker --------------------------------------------------------

interface RotationPickerProps {
  displayId: DisplayId;
  current: number;
  onRefresh: () => Promise<void>;
}

export function RotationPicker({ displayId, current, onRefresh }: RotationPickerProps) {
  const [applying, setApplying] = useState(false);

  async function selectRotation(deg: number) {
    if (deg === current) return;
    setApplying(true);
    try {
      await api.setDisplayRotation(displayId.adapter_luid, displayId.target_id, deg);
      await onRefresh();
      toast.success(`Rotation set to ${deg}°`);
    } catch (err) {
      toast.error(`Rotation failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  return (
    <div className="flex items-center gap-0.5">
      {ROTATION_OPTIONS.map((deg) => (
        <button
          key={deg}
          onClick={() => selectRotation(deg)}
          disabled={applying}
          title={`Rotate ${deg}°`}
          className={`flex items-center justify-center w-7 h-6 rounded text-[10px] border transition-colors disabled:opacity-50 ${
            current === deg
              ? "bg-blue-700 border-blue-600 text-white"
              : "border-zinc-700 text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"
          }`}
        >
          {deg === 0 ? <RotateCw size={9} /> : `${deg}°`}
        </button>
      ))}
    </div>
  );
}

// ---- Clone / Mirror Picker --------------------------------------------------

interface ClonePickerProps {
  displayId: DisplayId;
  currentCloneSourceKey: string | null;
  otherOutputs: OutputConfig[];
  displays: DisplayInfo[];
  onRefresh: () => Promise<void>;
}

function ClonePicker({ displayId, currentCloneSourceKey, otherOutputs, displays, onRefresh }: ClonePickerProps) {
  const [open, setOpen] = useState(false);
  const [applying, setApplying] = useState(false);

  const sourceName = currentCloneSourceKey
    ? displays.find((d) => displayKey(d.id) === currentCloneSourceKey)?.friendly_name ?? currentCloneSourceKey
    : null;

  async function selectSource(src: DisplayId | null) {
    setApplying(true);
    setOpen(false);
    try {
      if (src) {
        await api.setCloneSource(displayId.adapter_luid, displayId.target_id, src.adapter_luid, src.target_id);
        toast.success(`Mirroring enabled`);
      } else {
        await api.setCloneSource(displayId.adapter_luid, displayId.target_id, 0, 0);
        toast.success(`Extended mode restored`);
      }
      await onRefresh();
    } catch (err) {
      toast.error(`Clone failed: ${err}`);
    } finally {
      setApplying(false);
    }
  }

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        disabled={applying}
        title="Mirror / clone this display"
        className={`flex items-center gap-1 text-xs border rounded px-2 py-1 transition-colors disabled:opacity-50 ${
          currentCloneSourceKey
            ? "border-purple-600 text-purple-300 bg-purple-900/20 hover:bg-purple-900/40"
            : "border-zinc-700 text-zinc-400 hover:text-zinc-200 hover:border-zinc-500"
        }`}
      >
        <Copy size={9} />
        {applying ? "…" : currentCloneSourceKey ? `Mirrors: ${sourceName}` : "Extended"}
        <ChevronDown size={9} className={`transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-1 z-50 w-48 rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl">
          <button
            onClick={() => selectSource(null)}
            className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 transition-colors ${!currentCloneSourceKey ? "text-blue-400 font-medium" : "text-zinc-300"}`}
          >
            Extended (no mirror)
          </button>
          {otherOutputs.filter((o) => o.enabled).map((o) => {
            const name = displays.find((d) => displayKey(d.id) === displayKey(o.display_id))?.friendly_name ?? displayKey(o.display_id);
            const selected = currentCloneSourceKey === displayKey(o.display_id);
            return (
              <button
                key={displayKey(o.display_id)}
                onClick={() => selectSource(o.display_id)}
                className={`w-full text-left px-3 py-1.5 text-xs hover:bg-zinc-800 transition-colors ${selected ? "text-purple-400 font-medium" : "text-zinc-300"}`}
              >
                Mirror: {name}
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

  const { displays, layout, pending_confirmation, gdi_names, rotation_values, clone_pairs } = snapshot;
  const activeCount = displays.filter((d) => d.is_active).length;
  const currentLayout = draftLayout ?? layout;
  const isDirty = draftLayout !== null;

  // Reset draft when external layout changes (e.g. after confirmation revert)
  const prevLayoutRef = useRef("");
  useEffect(() => {
    const sig = JSON.stringify(layout.outputs.map(o => o.position));
    if (sig !== prevLayoutRef.current) {
      prevLayoutRef.current = sig;
      setDraftLayout(null);
    }
  }, [layout]);

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
      await api.applyLayout(normalizeLayout(draftLayout));
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
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Display Layout</h2>
        <button
          onClick={async () => {
            try { await api.refreshBackend(); } catch {}
            await onRefresh();
          }}
          className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
          title="Re-query all display state from Windows"
        >
          <RefreshCw size={12} /> Refresh
        </button>
      </div>

      <LayoutCanvas
        draft={currentLayout}
        displays={displays}
        rotations={rotation_values ?? {}}
        clonePairs={clone_pairs ?? {}}
        onDraftChange={setDraftLayout}
      />

      <div className="flex items-center gap-2 min-h-[28px]">
        {isDirty ? (
          <>
            <button onClick={applyDraft} disabled={applyingLayout || pending_confirmation}
              className="px-3 py-1.5 rounded text-xs font-medium bg-blue-700 hover:bg-blue-600 text-white transition-colors disabled:opacity-50">
              {applyingLayout ? "Applying…" : "Apply Layout"}
            </button>
            <button onClick={() => setDraftLayout(null)}
              className="px-3 py-1.5 rounded text-xs text-zinc-400 hover:text-zinc-200 border border-zinc-700 hover:border-zinc-500 transition-colors">
              Reset
            </button>
          </>
        ) : (
          <p className="text-xs text-zinc-600">Drag monitors to reposition, then click Apply Layout.</p>
        )}
      </div>

      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-zinc-400">Connected Displays</h2>
      </div>

      <div className="grid gap-3">
        {displays.map((display, index) => {
          const key = displayKey(display.id);
          const output = currentLayout.outputs.find(
            (o) =>
              o.display_id.adapter_luid === display.id.adapter_luid &&
              o.display_id.target_id === display.id.target_id
          );
          const gdiName = gdi_names?.[key];
          const dispNum = gdiDisplayNumber(gdiName) ?? String(index + 1);
          const isBusy = busy === key;

          return (
            <div key={key}
              className={`rounded-lg border p-4 transition-colors ${
                display.is_active ? "border-zinc-700 bg-zinc-900" : "border-zinc-800 bg-zinc-900/50 opacity-60"
              }`}
            >
              <div className="flex items-start gap-4">
                {/* Left: icon + info */}
                <div className="flex items-start gap-3 flex-1 min-w-0">
                  <Monitor size={20} className={`mt-0.5 shrink-0 ${display.is_active ? "text-blue-400" : "text-zinc-600"}`} />
                  <div className="min-w-0">
                    <div className="font-medium text-sm flex items-center gap-2 flex-wrap">
                      <span className="text-xs text-zinc-500 font-mono">#{dispNum}</span>
                      {display.friendly_name.replace(` ${display.id.target_id}`, '').trim() || 'Display'}
                      <span className="text-xs text-zinc-500 font-mono">{display.id.target_id}</span>
                    </div>
                    <div className="text-xs text-zinc-500 mt-0.5">
                      {display.resolution.width}×{display.resolution.height} @ {Math.round(display.refresh_rate_mhz / 1000)} Hz
                    </div>
                    {output && display.is_active && (
                      <div className="text-xs text-zinc-600 mt-0.5">
                        ({output.position.x}, {output.position.y})
                      </div>
                    )}
                  </div>
                </div>

                {/* Middle: DPI + Res (row 1) / Extended + Rotation (row 2) */}
                {display.is_active && (() => {
                  const isMirror = key in (clone_pairs ?? {});
                  return (
                    <div className="flex flex-col gap-1.5 shrink-0">
                      {/* Row 1: DPI + Resolution — hidden for mirrors (shares source's settings) */}
                      {isMirror ? (
                        <button
                          onClick={() => {/* future: show resolution compatibility popup */}}
                          className="flex items-center gap-1.5 text-[10px] border border-amber-600/60 bg-amber-950/40 text-amber-400 hover:bg-amber-950/70 rounded px-2 py-1 transition-colors"
                        >
                          <AlertTriangle size={11} className="shrink-0" />
                          Check valid shared resolutions!
                        </button>
                      ) : (
                        <div className="flex items-center gap-2 flex-wrap">
                          <DpiPicker displayId={display.id} currentDpi={snapshot.dpi_values?.[key]} onRefresh={onRefresh} />
                          {gdiName && <ResolutionPicker display={display} gdiName={gdiName} onRefresh={onRefresh} />}
                        </div>
                      )}
                      {/* Row 2: Extended picker always visible; Rotation hidden for mirrors */}
                      <div className="flex items-center gap-2 flex-wrap">
                        {output && (
                          <ClonePicker
                            displayId={display.id}
                            currentCloneSourceKey={clone_pairs?.[key] ?? null}
                            otherOutputs={layout.outputs.filter((o) => displayKey(o.display_id) !== key)}
                            displays={displays}
                            onRefresh={onRefresh}
                          />
                        )}
                        {!isMirror && (
                          <RotationPicker
                            displayId={display.id}
                            current={rotation_values?.[key] ?? 0}
                            onRefresh={onRefresh}
                          />
                        )}
                      </div>
                    </div>
                  );
                })()}

                {/* Right: Primary + Enable/Disable */}
                <div className="flex flex-col gap-1.5 items-end shrink-0">
                  {display.is_active && (
                    display.is_primary ? (
                      <button disabled
                        className="flex items-center gap-1 px-2 py-1.5 rounded text-xs font-medium bg-yellow-900/40 text-yellow-300 border border-yellow-700 cursor-default opacity-90">
                        ★ Primary
                      </button>
                    ) : (
                      <button onClick={() => makePrimary(display)}
                        disabled={isBusy || pending_confirmation}
                        className="flex items-center gap-1 px-2 py-1.5 rounded text-xs text-zinc-400 hover:text-yellow-300 border border-zinc-700 hover:border-yellow-600 transition-colors disabled:opacity-50">
                        ☆ Primary
                      </button>
                    )
                  )}
                  <button onClick={() => toggleDisplay(display)}
                    disabled={isBusy || (display.is_active && activeCount <= 1)}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
                      display.is_active
                        ? "bg-red-900/40 text-red-400 hover:bg-red-900/60 border border-red-900"
                        : "bg-green-900/40 text-green-400 hover:bg-green-900/60 border border-green-900"
                    } disabled:opacity-50`}
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
          <div className="text-center text-zinc-500 py-12 text-sm">No displays detected</div>
        )}
      </div>

      <div className="h-[200px]" />
    </div>
  );
}
