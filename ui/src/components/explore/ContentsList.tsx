// The list implements the ARIA grid "aria-activedescendant" pattern: keyboard focus stays on
// the grid element and rows are referenced by id, so rows/cells are intentionally not focusable
// and row clicks have no separate key handler (keys are handled by the grid). biome.json scopes
// the matching lint exceptions to this file only.
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, Cloud, File, Folder, FolderSymlink, Lock } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getBackend } from "../../lib/backend";
import { categoryVar } from "../../lib/colors";
import { formatBytes, formatCount, formatDate, formatPercent } from "../../lib/format";
import type { Metric, NodeRow, SortKey } from "../../lib/types";
import { Badge } from "../ui";
import type { MenuTarget } from "./TreemapView";

const PAGE = 200;
const ROW_H = 32;

interface Props {
  scanId: number;
  node: number;
  nodeName: string;
  viewTotal: number;
  metric: Metric;
  revision: number;
  onOpen: (row: NodeRow) => void;
  onUp: () => void;
  onMenu: (target: MenuTarget, x: number, y: number) => void;
}

/** Virtualised, sortable list of a folder's contents (keyboard: arrows, Enter, Backspace, menu key). */
export function ContentsList({
  scanId,
  node,
  nodeName,
  viewTotal,
  metric,
  revision,
  onOpen,
  onUp,
  onMenu,
}: Props) {
  const { t } = useTranslation();
  const sizeKey: SortKey = metric === "allocated" ? "allocated" : "size";
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: sizeKey, desc: true });
  const [total, setTotal] = useState<number | null>(null);
  const [pages, setPages] = useState<Map<number, NodeRow[]>>(new Map());
  const [active, setActive] = useState(0);
  const loadingPages = useRef(new Set<number>());
  const scrollRef = useRef<HTMLDivElement>(null);
  const generation = useRef(0);

  // Keep the size column in step with the chosen metric.
  useEffect(() => {
    setSort((s) => (s.key === "size" || s.key === "allocated" ? { ...s, key: sizeKey } : s));
  }, [sizeKey]);

  // Reset when the folder, sort or data changes. Each reset starts a new epoch, which always
  // (re)loads the first page — a response from an older epoch is discarded by `generation`.
  const [epoch, setEpoch] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: revision intentionally triggers a refresh
  useEffect(() => {
    generation.current++;
    loadingPages.current.clear();
    setPages(new Map());
    setTotal(null);
    setEpoch((e) => e + 1);
  }, [scanId, node, sort, revision]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset position whenever the folder changes
  useEffect(() => {
    setActive(0);
    scrollRef.current?.scrollTo({ top: 0 });
  }, [node]);

  const loadPage = useCallback(
    async (p: number) => {
      if (loadingPages.current.has(p)) return;
      loadingPages.current.add(p);
      const gen = generation.current;
      try {
        const res = await (await getBackend()).children(scanId, {
          node,
          sort: sort.key,
          descending: sort.desc,
          offset: p * PAGE,
          limit: PAGE,
        });
        if (gen !== generation.current) return;
        setTotal(res.total);
        setPages((prev) => new Map(prev).set(p, res.rows));
      } catch {
        loadingPages.current.delete(p);
      }
    },
    [scanId, node, sort],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: epoch marks a reset that needs page 0
  useEffect(() => {
    void loadPage(0);
  }, [epoch, loadPage]);

  const count = total ?? 0;
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 12,
  });
  const items = virtualizer.getVirtualItems();

  useEffect(() => {
    for (const it of items) {
      const p = Math.floor(it.index / PAGE);
      if (!pages.has(p)) void loadPage(p);
    }
  }, [items, pages, loadPage]);

  const rowAt = (i: number) => pages.get(Math.floor(i / PAGE))?.[i % PAGE];
  const activeRow = rowAt(active);
  const listId = `list-${scanId}-${node}`;

  const move = (to: number) => {
    const next = Math.max(0, Math.min(count - 1, to));
    setActive(next);
    virtualizer.scrollToIndex(next, { align: "auto" });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const pageRows = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 320) / ROW_H) - 1);
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(active + 1),
      ArrowUp: () => move(active - 1),
      PageDown: () => move(active + pageRows),
      PageUp: () => move(active - pageRows),
      Home: () => move(0),
      End: () => move(count - 1),
      Enter: () => activeRow?.isDir && !activeRow.isLink && onOpen(activeRow),
      Backspace: onUp,
    };
    const action = keys[e.key];
    if (action) {
      e.preventDefault();
      action();
    } else if ((e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) && activeRow) {
      e.preventDefault();
      const el = document.getElementById(`${listId}-row-${active}`)?.getBoundingClientRect();
      onMenu(
        { id: activeRow.id, name: activeRow.name, isDir: activeRow.isDir },
        (el?.left ?? 0) + 40,
        (el?.top ?? 0) + ROW_H,
      );
    }
  };

  const header = (key: SortKey, label: string, className: string) => {
    const on = sort.key === key;
    return (
      <div
        role="columnheader"
        aria-sort={on ? (sort.desc ? "descending" : "ascending") : "none"}
        className={className}
      >
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded px-1 hover:text-ink"
          title={t("list.sortBy", { column: label })}
          onClick={() => setSort({ key, desc: on ? !sort.desc : key !== "name" })}
        >
          {label}
          {on && (sort.desc ? <ArrowDown size={12} aria-hidden /> : <ArrowUp size={12} aria-hidden />)}
        </button>
      </div>
    );
  };

  const cols = "grid grid-cols-[minmax(0,1fr)_88px_84px_72px_104px] items-center gap-2 px-3";

  return (
    <div
      ref={scrollRef}
      role="grid"
      aria-label={t("list.label", { name: nodeName })}
      aria-rowcount={count + 1}
      aria-activedescendant={count ? `${listId}-row-${active}` : undefined}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="h-full min-h-0 overflow-y-auto focus-visible:outline-offset-[-2px]"
    >
      <div role="rowgroup" className="sticky top-0 z-10 bg-surface">
        <div
          role="row"
          aria-rowindex={1}
          className={`${cols} h-8 border-b border-line text-xs font-semibold text-ink-2`}
        >
          {header("name", t("list.name"), "")}
          <div role="columnheader" className="px-1">
            {t("list.share")}
          </div>
          {header(sizeKey, t("list.size"), "text-right")}
          {header("files", t("list.files"), "text-right")}
          {header("modified", t("list.modified"), "text-right")}
        </div>
      </div>
      {total === 0 && <p className="p-4 text-ink-3">{t("list.empty")}</p>}
      {total === null && (
        <div className="flex flex-col gap-2 p-3">
          <span className="sr-only">{t("list.loading")}</span>
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="skeleton h-5" style={{ width: `${90 - i * 9}%` }} />
          ))}
        </div>
      )}
      <div role="rowgroup" style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {items.map((it) => {
          const row = rowAt(it.index);
          const isActive = it.index === active;
          return (
            <div
              key={it.key}
              id={`${listId}-row-${it.index}`}
              role="row"
              aria-rowindex={it.index + 2}
              aria-selected={isActive}
              className={`${cols} absolute right-0 left-0 cursor-default text-[13px] ${isActive ? "bg-pressed" : "hover:bg-hover"}`}
              style={{ height: ROW_H, transform: `translateY(${it.start}px)` }}
              onClick={() => setActive(it.index)}
              onDoubleClick={() => row?.isDir && !row.isLink && onOpen(row)}
              onContextMenu={(e) => {
                if (!row) return;
                e.preventDefault();
                setActive(it.index);
                onMenu({ id: row.id, name: row.name, isDir: row.isDir }, e.clientX, e.clientY);
              }}
            >
              {row ? (
                <Cells row={row} metric={metric} viewTotal={viewTotal} />
              ) : (
                <div className="skeleton col-span-5 h-4" />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function Cells({ row, metric, viewTotal }: { row: NodeRow; metric: Metric; viewTotal: number }) {
  const { t } = useTranslation();
  const value = metric === "allocated" ? row.alloc : row.size;
  const share = viewTotal > 0 && !row.isLink && !row.hardlink ? value / viewTotal : 0;
  const Icon = row.isLink ? FolderSymlink : row.isDir ? Folder : row.cloud ? Cloud : File;
  return (
    <>
      <div role="gridcell" className="flex min-w-0 items-center gap-2">
        <Icon
          size={16}
          aria-hidden
          className="shrink-0"
          style={{ color: row.isDir ? "var(--text-secondary)" : categoryVar(row.category ?? "other") }}
        />
        <span className={`truncate ${row.hidden ? "text-ink-2" : ""}`} title={row.name}>
          {row.name}
        </span>
        {row.cloud && <Badge title={t("explore.badges.cloudHint")}>{t("explore.badges.cloud")}</Badge>}
        {row.isLink && <Badge title={t("explore.badges.linkHint")}>{t("explore.badges.link")}</Badge>}
        {row.hardlink && (
          <Badge title={t("explore.badges.hardlinkHint")}>{t("explore.badges.hardlink")}</Badge>
        )}
        {row.unreadable && (
          <Badge tone="warning" title={t("explore.badges.unreadableHint", { name: "" })}>
            <Lock size={11} aria-hidden />
            {t("explore.badges.unreadable")}
          </Badge>
        )}
      </div>
      <div role="gridcell" className="flex items-center gap-1.5 px-1" title={formatPercent(share)}>
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-track">
          <div
            className="h-full rounded-full bg-accent/70"
            style={{ width: `${Math.min(100, share * 100)}%` }}
          />
        </div>
      </div>
      <div role="gridcell" className="tabular text-right">
        {row.isLink ? "–" : formatBytes(value)}
      </div>
      <div role="gridcell" className="tabular text-right text-ink-2">
        {row.isDir && !row.isLink ? formatCount(row.files) : ""}
      </div>
      <div role="gridcell" className="tabular text-right text-ink-3">
        {formatDate(row.mtime)}
      </div>
    </>
  );
}
