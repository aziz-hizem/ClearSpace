import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getBackend } from "../../lib/backend";
import { inkOn, type Palette, readPalette } from "../../lib/colors";
import { ageBucket, formatBytes, formatCount, formatDate, formatPercent } from "../../lib/format";
import { CATEGORY_KEYS, CellFlag, decodeTreemap, hitTest, type TreemapCells } from "../../lib/treemapData";
import type { Category, Metric, NodeRow } from "../../lib/types";
import type { ColorBy } from "../../state/store";
import { Spinner } from "../ui";

export interface MenuTarget {
  id: number;
  name: string;
  isDir: boolean;
}

interface Props {
  scanId: number;
  node: number;
  /** Size of the current view's root, for "% of this view". */
  viewTotal: number;
  viewPath: string;
  metric: Metric;
  colorBy: ColorBy;
  revision: number;
  theme: string;
  highlight: Category | null;
  /** A context menu is open: hide the hover tooltip so it can't cover the menu. */
  menuOpen: boolean;
  onZoom: (node: number) => void;
  onUp: () => void;
  onMenu: (target: MenuTarget, x: number, y: number) => void;
}

const LABEL_MIN_W = 64;
const LABEL_MIN_H = 20;

/**
 * Canvas treemap. The base layer is drawn once per layout/colour change; hover and focus are
 * drawn on a separate overlay so pointer movement stays at 60 fps even with 60k cells.
 */
export function TreemapView(props: Props) {
  const { scanId, node, metric, colorBy, revision, theme, highlight, viewTotal } = props;
  const { t } = useTranslation();
  const wrapRef = useRef<HTMLDivElement>(null);
  const baseRef = useRef<HTMLCanvasElement>(null);
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [cells, setCells] = useState<TreemapCells | null>(null);
  const [rows, setRows] = useState<Map<number, NodeRow>>(new Map());
  const [loading, setLoading] = useState(true);
  const [hover, setHover] = useState<{ index: number; x: number; y: number } | null>(null);
  const [focusIndex, setFocusIndex] = useState(-1);

  // Track the container size (debounced so window drags don't flood the backend).
  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ro = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      clearTimeout(timer);
      timer = setTimeout(() => setSize({ w: Math.floor(width), h: Math.floor(height) }), 120);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      clearTimeout(timer);
    };
  }, []);

  // Fetch the layout, then names/sizes for the cells big enough to label.
  // biome-ignore lint/correctness/useExhaustiveDependencies: revision intentionally triggers a refresh
  useEffect(() => {
    if (size.w < 10 || size.h < 10) return;
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const backend = await getBackend();
        const buf = await backend.treemap(scanId, {
          node,
          width: size.w,
          height: size.h,
          metric,
          maxDepth: 12,
        });
        if (cancelled) return;
        const decoded = decodeTreemap(buf);
        const wanted: number[] = [];
        for (let i = 0; i < decoded.count && wanted.length < 4000; i++) {
          const flags = decoded.flags[i] ?? 0;
          const big = (decoded.w[i] ?? 0) >= LABEL_MIN_W && (decoded.h[i] ?? 0) >= LABEL_MIN_H;
          if (flags & CellFlag.Header || big || (decoded.depth[i] ?? 0) === 1)
            wanted.push(decoded.node[i] ?? 0);
        }
        const list = wanted.length ? await backend.rows(scanId, wanted) : [];
        if (cancelled) return;
        setRows(new Map(list.map((r) => [r.id, r])));
        setCells(decoded);
        setFocusIndex(-1);
      } catch {
        if (!cancelled) setCells(null);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [scanId, node, metric, size, revision]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: theme change must re-read CSS colours
  const palette = useMemo(() => readPalette(), [theme]);

  // Base layer.
  useEffect(() => {
    const canvas = baseRef.current;
    if (!canvas || !cells) return;
    drawBase(canvas, size, cells, rows, palette, colorBy, highlight);
  }, [cells, rows, palette, colorBy, highlight, size]);

  // Overlay: hover + keyboard focus rings.
  useEffect(() => {
    const canvas = overlayRef.current;
    if (!canvas) return;
    const ctx = prepare(canvas, size);
    if (!ctx || !cells) return;
    const ring = (i: number, color: string, width: number) => {
      const x = cells.x[i] ?? 0;
      const y = cells.y[i] ?? 0;
      const w = cells.w[i] ?? 0;
      const h = cells.h[i] ?? 0;
      ctx.lineWidth = width + 2;
      ctx.strokeStyle = palette.surface;
      ctx.strokeRect(x + width / 2, y + width / 2, Math.max(0, w - width), Math.max(0, h - width));
      ctx.lineWidth = width;
      ctx.strokeStyle = color;
      ctx.strokeRect(x + width / 2, y + width / 2, Math.max(0, w - width), Math.max(0, h - width));
    };
    if (focusIndex >= 0) ring(focusIndex, palette.accent, 3);
    if (hover && hover.index >= 0) ring(hover.index, palette.ink, 2);
  }, [hover, focusIndex, cells, palette, size]);

  const topLevel = useMemo(() => {
    const out: number[] = [];
    if (cells) for (let i = 0; i < cells.count; i++) if (cells.depth[i] === 1) out.push(i);
    return out;
  }, [cells]);

  const hoveredRow = hover && cells ? rows.get(cells.node[hover.index] ?? -1) : undefined;
  const [lazyRow, setLazyRow] = useState<NodeRow | null>(null);
  const hoveredNode = hover && cells ? (cells.node[hover.index] ?? -1) : -1;
  // Fetch details for unlabelled cells on hover (cached in `rows` afterwards).
  useEffect(() => {
    if (hoveredNode < 0 || rows.has(hoveredNode)) return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      const [r] = await (await getBackend()).rows(scanId, [hoveredNode]);
      if (!cancelled && r) setLazyRow(r);
    }, 40);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [hoveredNode, rows, scanId]);
  const tooltipRow = hoveredRow ?? (lazyRow?.id === hoveredNode ? lazyRow : null);

  const pointAt = (e: React.PointerEvent | React.MouseEvent) => {
    const r = wrapRef.current?.getBoundingClientRect();
    return r ? { x: e.clientX - r.left, y: e.clientY - r.top } : { x: 0, y: 0 };
  };

  const targetFor = useCallback(
    (index: number): MenuTarget | null => {
      if (!cells || index < 0) return null;
      const id = cells.node[index] ?? 0;
      const row = rows.get(id);
      return { id, name: row?.name ?? "", isDir: ((cells.flags[index] ?? 0) & CellFlag.Dir) !== 0 };
    },
    [cells, rows],
  );

  /** Clicking zooms one level: into the top-level folder under the pointer. */
  const activate = (index: number) => {
    if (!cells || index < 0) return;
    const x = (cells.x[index] ?? 0) + 1;
    const y = (cells.y[index] ?? 0) + 1;
    const top = topLevel.find((i) => {
      const cx = cells.x[i] ?? 0;
      const cy = cells.y[i] ?? 0;
      return x >= cx && y >= cy && x < cx + (cells.w[i] ?? 0) && y < cy + (cells.h[i] ?? 0);
    });
    if (top !== undefined && (cells.flags[top] ?? 0) & CellFlag.Dir) props.onZoom(cells.node[top] ?? 0);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!cells) return;
    const pos = topLevel.indexOf(focusIndex);
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      setFocusIndex(topLevel[Math.min(topLevel.length - 1, pos + 1)] ?? -1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      setFocusIndex(topLevel[Math.max(0, pos - 1)] ?? -1);
    } else if (e.key === "Home") {
      e.preventDefault();
      setFocusIndex(topLevel[0] ?? -1);
    } else if (e.key === "End") {
      e.preventDefault();
      setFocusIndex(topLevel[topLevel.length - 1] ?? -1);
    } else if (e.key === "Enter" && focusIndex >= 0) {
      e.preventDefault();
      activate(focusIndex);
    } else if (e.key === "Backspace") {
      e.preventDefault();
      props.onUp();
    } else if ((e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) && focusIndex >= 0) {
      e.preventDefault();
      const target = targetFor(focusIndex);
      const r = wrapRef.current?.getBoundingClientRect();
      if (target && r)
        props.onMenu(
          target,
          r.left + (cells.x[focusIndex] ?? 0) + 12,
          r.top + (cells.y[focusIndex] ?? 0) + 12,
        );
    }
  };

  const largest = topLevel
    .slice(0, 3)
    .map((i) => rows.get(cells?.node[i] ?? -1))
    .filter((r): r is NodeRow => !!r)
    .map((r) => `${r.name} ${formatBytes(metric === "allocated" ? r.alloc : r.size)}`)
    .join(", ");
  const focusedRow = focusIndex >= 0 && cells ? rows.get(cells.node[focusIndex] ?? -1) : undefined;

  return (
    <div
      ref={wrapRef}
      className="relative h-full min-h-0 w-full overflow-hidden rounded-card border border-line bg-sunken"
      role="application"
      aria-roledescription="treemap"
      aria-label={t("explore.mapLabel", { path: props.viewPath, largest: largest || "–" })}
      aria-describedby="treemap-hint"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the map is keyboard-operable (arrows, Enter, Backspace)
      tabIndex={0}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (focusIndex < 0 && topLevel.length) setFocusIndex(topLevel[0] ?? -1);
      }}
      onPointerMove={(e) => {
        if (!cells) return;
        const p = pointAt(e);
        const index = hitTest(cells, p.x, p.y);
        setHover(index >= 0 ? { index, ...p } : null);
      }}
      onPointerLeave={() => setHover(null)}
      onClick={(e) => {
        if (!cells) return;
        const p = pointAt(e);
        activate(hitTest(cells, p.x, p.y));
      }}
      onContextMenu={(e) => {
        if (!cells) return;
        e.preventDefault();
        const p = pointAt(e);
        const target = targetFor(hitTest(cells, p.x, p.y));
        if (target) props.onMenu(target, e.clientX, e.clientY);
      }}
    >
      <canvas ref={baseRef} className="absolute inset-0" aria-hidden />
      <canvas ref={overlayRef} className="pointer-events-none absolute inset-0" aria-hidden />
      <p id="treemap-hint" className="sr-only">
        {t("explore.mapHint")}
      </p>
      {focusedRow && (
        <p className="sr-only" aria-live="polite">
          {focusedRow.name}, {formatBytes(metric === "allocated" ? focusedRow.alloc : focusedRow.size)}
        </p>
      )}
      {loading && !cells && (
        <div className="absolute inset-0 flex items-center justify-center">
          <Spinner label={t("explore.loadingMap")} />
        </div>
      )}
      {!loading && cells?.count === 0 && (
        <div className="absolute inset-0 flex items-center justify-center text-ink-2">
          {t("explore.mapEmpty")}
        </div>
      )}
      {hover && tooltipRow && !props.menuOpen && (
        <Tooltip
          row={tooltipRow}
          x={hover.x}
          y={hover.y}
          bounds={size}
          metric={metric}
          viewTotal={viewTotal}
        />
      )}
    </div>
  );
}

function Tooltip({
  row,
  x,
  y,
  bounds,
  metric,
  viewTotal,
}: {
  row: NodeRow;
  x: number;
  y: number;
  bounds: { w: number; h: number };
  metric: Metric;
  viewTotal: number;
}) {
  const { t } = useTranslation();
  const value = metric === "allocated" ? row.alloc : row.size;
  const W = 280;
  const left = x + 16 + W > bounds.w ? Math.max(4, x - W - 12) : x + 16;
  const top = Math.min(Math.max(4, y + 16), Math.max(4, bounds.h - 120));
  return (
    <div
      className="pointer-events-none absolute z-10 rounded-lg border border-line bg-raised px-3 py-2 text-[13px] shadow-card"
      style={{ left, top, width: W }}
    >
      <p className="truncate font-semibold">{row.name}</p>
      <p className="tabular">
        {formatBytes(value)}
        {viewTotal > 0 && (
          <span className="text-ink-3">
            {" "}
            · {t("explore.ofView", { percent: formatPercent(value / viewTotal) })}
          </span>
        )}
      </p>
      <p className="text-ink-2">
        {row.isDir
          ? t("explore.folderContains", {
              files: t("common.files", { count: row.files, formatted: formatCount(row.files) }),
            })
          : t(`types.category.${row.category ?? "other"}`)}
      </p>
      {row.mtime > 0 && (
        <p className="text-ink-3">{t("explore.modified", { date: formatDate(row.mtime) })}</p>
      )}
      {row.cloud && <p className="text-ink-3">{t("explore.badges.cloud")}</p>}
    </div>
  );
}

function prepare(canvas: HTMLCanvasElement, size: { w: number; h: number }) {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(size.w * dpr));
  const h = Math.max(1, Math.round(size.h * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
    canvas.style.width = `${size.w}px`;
    canvas.style.height = `${size.h}px`;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, size.w, size.h);
  return ctx;
}

function fitText(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (ctx.measureText(`${text.slice(0, mid)}…`).width <= max) lo = mid;
    else hi = mid - 1;
  }
  return lo > 0 ? `${text.slice(0, lo)}…` : "";
}

function drawBase(
  canvas: HTMLCanvasElement,
  size: { w: number; h: number },
  cells: TreemapCells,
  rows: Map<number, NodeRow>,
  palette: Palette,
  colorBy: ColorBy,
  highlight: Category | null,
) {
  const ctx = prepare(canvas, size);
  if (!ctx) return;
  ctx.fillStyle = palette.sunken;
  ctx.fillRect(0, 0, size.w, size.h);
  const now = Date.now() / 60_000;
  const highlightIndex = highlight ? CATEGORY_KEYS.indexOf(highlight) : -1;
  ctx.textBaseline = "middle";
  const labels: number[] = [];

  for (let i = 0; i < cells.count; i++) {
    const x = cells.x[i] ?? 0;
    const y = cells.y[i] ?? 0;
    const w = cells.w[i] ?? 0;
    const h = cells.h[i] ?? 0;
    const flags = cells.flags[i] ?? 0;
    if (flags & CellFlag.Dir) {
      ctx.globalAlpha = 1;
      ctx.fillStyle = palette.folder[((cells.depth[i] ?? 1) - 1) % palette.folder.length] ?? palette.sunken;
      ctx.fillRect(x, y, w, h);
      if (flags & CellFlag.Header) labels.push(i);
      continue;
    }
    const cat = cells.category[i] ?? 8;
    const color =
      colorBy === "type"
        ? (palette.category[cat] ?? palette.category[8])
        : palette.age[ageBucket(cells.mtime[i] ?? 0, now)];
    ctx.globalAlpha = highlightIndex >= 0 && cat !== highlightIndex ? 0.22 : flags & CellFlag.Cloud ? 0.5 : 1;
    ctx.fillStyle = color ?? "#888";
    // A 2px surface gap between neighbouring files keeps boxes distinct.
    if (w > 4 && h > 4) ctx.fillRect(x + 1, y + 1, w - 2, h - 2);
    else ctx.fillRect(x, y, w, h);
    if (w >= LABEL_MIN_W && h >= LABEL_MIN_H) labels.push(i);
  }
  ctx.globalAlpha = 1;

  // Labels last so they are never painted over.
  for (const i of labels) {
    const row = rows.get(cells.node[i] ?? -1);
    if (!row) continue;
    const x = cells.x[i] ?? 0;
    const y = cells.y[i] ?? 0;
    const w = cells.w[i] ?? 0;
    const h = cells.h[i] ?? 0;
    const isDir = ((cells.flags[i] ?? 0) & CellFlag.Dir) !== 0;
    if (isDir) {
      ctx.font = "600 11.5px 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";
      ctx.fillStyle = palette.headerText;
      const text = fitText(ctx, row.name, w - 10);
      if (text) ctx.fillText(text, x + 5, y + 8.5);
    } else {
      const dimmed = highlight && CATEGORY_KEYS[cells.category[i] ?? 8] !== highlight;
      if (dimmed) continue;
      const cat = cells.category[i] ?? 8;
      const fill = (palette.category[cat] ?? "#888").trim();
      ctx.font = "11.5px 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";
      ctx.fillStyle =
        colorBy === "type" ? inkOn(fill) : inkOn(palette.age[ageBucket(cells.mtime[i] ?? 0, now)] ?? "#888");
      const text = fitText(ctx, row.name, w - 10);
      if (text) ctx.fillText(text, x + 5, y + Math.min(h / 2, 11));
    }
  }
}
