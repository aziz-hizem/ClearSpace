import { Map as MapIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Breadcrumbs } from "../components/explore/Breadcrumbs";
import { ContentsList } from "../components/explore/ContentsList";
import { ContextMenu, type MenuAction } from "../components/explore/ContextMenu";
import { type MenuTarget, TreemapView } from "../components/explore/TreemapView";
import { TypeBreakdown } from "../components/explore/TypeBreakdown";
import { Button, EmptyState, Segmented, Spinner } from "../components/ui";
import { getBackend } from "../lib/backend";
import { categoryVar } from "../lib/colors";
import { useAppliedTheme } from "../lib/theme";
import { CATEGORY_KEYS } from "../lib/treemapData";
import type { Category, NodeInfo } from "../lib/types";
import { errorMessage, useExploreScan, useStore } from "../state/store";

export function Explore() {
  const { t } = useTranslation();
  const scan = useExploreScan();
  const explore = useStore((s) => s.explore);
  const go = useStore((s) => s.go);
  const setNode = useStore((s) => s.setExploreNode);
  const setMetric = useStore((s) => s.setMetric);
  const setColorBy = useStore((s) => s.setColorBy);
  const toast = useStore((s) => s.toast);
  const theme = useAppliedTheme(useStore((s) => s.themePref));
  const [info, setInfo] = useState<NodeInfo | null>(null);
  const [revision, setRevision] = useState(0);
  const [tab, setTab] = useState<"contents" | "types">("contents");
  const [highlight, setHighlight] = useState<Category | null>(null);
  const [menu, setMenu] = useState<{ target: MenuTarget; x: number; y: number } | null>(null);

  const scanId = scan?.scanId ?? null;
  const running = scan?.progress.status === "running";

  // While a scan runs, refresh every few seconds; refresh once more when it ends.
  useEffect(() => {
    if (!running) {
      setRevision((r) => r + 1);
      return;
    }
    const timer = setInterval(() => setRevision((r) => r + 1), 2500);
    return () => clearInterval(timer);
  }, [running]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: revision refreshes sizes in the header
  useEffect(() => {
    if (scanId === null) return;
    let cancelled = false;
    void getBackend()
      .then((b) => b.nodeInfo(scanId, explore.node))
      .then((i) => !cancelled && setInfo(i))
      .catch(() => !cancelled && setNode(0));
    return () => {
      cancelled = true;
    };
  }, [scanId, explore.node, revision, setNode]);

  const up = useCallback(() => {
    if (info?.parent != null) setNode(info.parent);
  }, [info, setNode]);

  const openMenu = useCallback((target: MenuTarget, x: number, y: number) => setMenu({ target, x, y }), []);

  if (!scan || scanId === null) {
    return (
      <EmptyState
        icon={<MapIcon size={28} aria-hidden />}
        title={t("explore.emptyTitle")}
        body={t("explore.emptyBody")}
      >
        <Button variant="primary" onClick={() => go("overview")}>
          {t("explore.goOverview")}
        </Button>
      </EmptyState>
    );
  }

  const viewTotal = info ? (explore.metric === "allocated" ? info.node.alloc : info.node.size) : 0;

  const actions = (target: MenuTarget): MenuAction[] => {
    const list: MenuAction[] = [
      {
        id: "openInExplorer",
        run: () =>
          void getBackend().then((b) =>
            b.openInExplorer(scanId, target.id).catch((e) => toast(errorMessage(e), "error")),
          ),
      },
      {
        id: "copyPath",
        run: () =>
          void getBackend()
            .then((b) => b.nodeInfo(scanId, target.id))
            .then((i) => navigator.clipboard.writeText(i.path))
            .then(() => toast(t("menu.pathCopied")))
            .catch(() => toast(t("menu.copyFailed"), "error")),
      },
    ];
    if (target.isDir) list.push({ id: "zoomInto", run: () => setNode(target.id) });
    return list;
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 px-6 pt-5 pb-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          {info ? (
            <Breadcrumbs crumbs={info.ancestors} onGo={setNode} />
          ) : (
            <div className="skeleton h-6 w-64" />
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3 text-[13px]">
          <span className="text-ink-2">{t("explore.metric")}</span>
          <Segmented
            label={t("explore.metric")}
            value={explore.metric}
            onChange={setMetric}
            options={[
              {
                value: "allocated",
                label: t("explore.metricAllocated"),
                hint: t("explore.metricAllocatedHint"),
              },
              { value: "logical", label: t("explore.metricLogical"), hint: t("explore.metricLogicalHint") },
            ]}
          />
          <span className="text-ink-2">{t("explore.colorBy")}</span>
          <Segmented
            label={t("explore.colorBy")}
            value={explore.colorBy}
            onChange={setColorBy}
            options={[
              { value: "type", label: t("explore.colorType") },
              { value: "age", label: t("explore.colorAge") },
            ]}
          />
        </div>
      </div>

      {running && (
        <p className="flex items-center gap-2 text-[13px] text-ink-2" role="status">
          <Spinner size={14} /> {t("explore.partial")}
        </p>
      )}

      <div className="flex min-h-0 flex-[1.35] flex-col gap-2">
        <div className="min-h-0 flex-1">
          <TreemapView
            scanId={scanId}
            node={explore.node}
            viewTotal={viewTotal}
            viewPath={info?.path ?? ""}
            metric={explore.metric}
            colorBy={explore.colorBy}
            revision={revision}
            theme={theme}
            highlight={highlight}
            menuOpen={menu !== null}
            onZoom={setNode}
            onUp={up}
            onMenu={openMenu}
          />
        </div>
        <Legend colorBy={explore.colorBy} onHighlight={setHighlight} />
      </div>

      <div className="flex min-h-0 flex-1 gap-3">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-card border border-line bg-surface">
          <div
            role="tablist"
            aria-label={t("explore.title")}
            className="flex shrink-0 gap-1 border-b border-line px-2 pt-1.5"
          >
            {(["contents", "types"] as const).map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                id={`tab-${k}`}
                aria-selected={tab === k}
                aria-controls={`panel-${k}`}
                tabIndex={tab === k ? 0 : -1}
                onClick={() => setTab(k)}
                onKeyDown={(e) => {
                  if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                    e.preventDefault();
                    const next = tab === "contents" ? "types" : "contents";
                    setTab(next);
                    requestAnimationFrame(() => document.getElementById(`tab-${next}`)?.focus());
                  }
                }}
                className={`-mb-px border-b-2 px-3 py-1.5 text-[13px] ${
                  tab === k
                    ? "border-accent font-semibold text-ink"
                    : "border-transparent text-ink-2 hover:text-ink"
                }`}
              >
                {t(k === "contents" ? "explore.tabContents" : "explore.tabTypes")}
              </button>
            ))}
          </div>
          <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`} className="min-h-0 flex-1">
            {tab === "contents" ? (
              <ContentsList
                scanId={scanId}
                node={explore.node}
                nodeName={info?.node.name ?? ""}
                viewTotal={viewTotal}
                metric={explore.metric}
                revision={revision}
                onOpen={(row) => setNode(row.id)}
                onUp={up}
                onMenu={openMenu}
              />
            ) : (
              <TypeBreakdown
                scanId={scanId}
                node={explore.node}
                metric={explore.metric}
                revision={revision}
                onHighlight={setHighlight}
              />
            )}
          </div>
        </div>
      </div>

      {menu && (
        <ContextMenu
          target={menu.target}
          x={menu.x}
          y={menu.y}
          actions={actions(menu.target)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

/** Always-visible legend, so colour is never the only way to tell types (or ages) apart. */
function Legend({
  colorBy,
  onHighlight,
}: {
  colorBy: "type" | "age";
  onHighlight: (c: Category | null) => void;
}) {
  const { t } = useTranslation();
  if (colorBy === "age") {
    return (
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-ink-2">
        <span className="font-medium">{t("explore.ageLegend")}</span>
        {[0, 1, 2, 3, 4].map((i) => (
          <span key={i} className="inline-flex items-center gap-1.5">
            <span aria-hidden className="size-3 rounded-sm" style={{ background: `var(--age-${i})` }} />
            {t(`explore.age.${i}`)}
          </span>
        ))}
      </div>
    );
  }
  return (
    <ul
      className="flex flex-wrap items-center gap-x-1 gap-y-1 text-xs text-ink-2"
      onMouseLeave={() => onHighlight(null)}
    >
      {CATEGORY_KEYS.map((c) => (
        <li key={c}>
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 hover:bg-hover hover:text-ink"
            onMouseEnter={() => onHighlight(c)}
            onFocus={() => onHighlight(c)}
            onBlur={() => onHighlight(null)}
          >
            <span aria-hidden className="size-3 rounded-sm" style={{ background: categoryVar(c) }} />
            {t(`types.category.${c}`)}
          </button>
        </li>
      ))}
    </ul>
  );
}
