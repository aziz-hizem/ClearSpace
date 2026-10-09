import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getBackend } from "../../lib/backend";
import { categoryVar } from "../../lib/colors";
import { formatBytes, formatCount, formatPercent } from "../../lib/format";
import type { TypeBreakdown as Breakdown, Category, Metric } from "../../lib/types";

/** Per-category totals (the table view for the map's colours) plus top extensions. */
export function TypeBreakdown({
  scanId,
  node,
  metric,
  revision,
  onHighlight,
}: {
  scanId: number;
  node: number;
  metric: Metric;
  revision: number;
  onHighlight: (c: Category | null) => void;
}) {
  const { t } = useTranslation();
  const [data, setData] = useState<Breakdown | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: revision intentionally triggers a refresh
  useEffect(() => {
    let cancelled = false;
    void getBackend()
      .then((b) => b.typeBreakdown(scanId, node))
      .then((d) => !cancelled && setData(d))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [scanId, node, revision]);

  if (!data) return <div className="skeleton m-4 h-40" />;
  const value = (x: { size: number; alloc: number }) => (metric === "allocated" ? x.alloc : x.size);
  const cats = [...data.categories].sort((a, b) => value(b) - value(a));
  const total = cats.reduce((s, c) => s + value(c), 0);

  return (
    <div className="h-full overflow-y-auto p-4">
      <h3 className="font-semibold">{t("types.title")}</h3>
      <p className="mb-3 text-xs text-ink-3">{t("types.hint")}</p>
      <ul className="flex flex-col gap-0.5" onMouseLeave={() => onHighlight(null)}>
        {cats.map((c) => (
          <li key={c.category}>
            <button
              type="button"
              className="w-full rounded px-2 py-1.5 text-left hover:bg-hover focus:bg-hover"
              onMouseEnter={() => onHighlight(c.category)}
              onFocus={() => onHighlight(c.category)}
              onBlur={() => onHighlight(null)}
            >
              <span className="flex items-center gap-2 text-[13px]">
                <span
                  aria-hidden
                  className="size-3 shrink-0 rounded-sm"
                  style={{ background: categoryVar(c.category) }}
                />
                <span className="flex-1 truncate">{t(`types.category.${c.category}`)}</span>
                <span className="tabular font-medium">{formatBytes(value(c))}</span>
              </span>
              <span className="mt-1 flex items-center gap-2 pl-5 text-xs text-ink-3">
                <span className="h-1 flex-1 overflow-hidden rounded-full bg-track">
                  <span
                    className="block h-full rounded-full"
                    style={{
                      width: `${total ? (value(c) / total) * 100 : 0}%`,
                      background: categoryVar(c.category),
                    }}
                  />
                </span>
                <span className="tabular w-12 text-right">{formatPercent(total ? value(c) / total : 0)}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      <h3 className="mt-5 mb-2 text-[13px] font-semibold">{t("types.extensions")}</h3>
      <table className="w-full text-[13px]">
        <tbody>
          {data.extensions
            .slice()
            .sort((a, b) => value(b) - value(a))
            .slice(0, 12)
            .map((e) => (
              <tr key={e.ext} className="border-t border-line">
                <td className="py-1.5">
                  <span
                    aria-hidden
                    className="mr-2 inline-block size-2 rounded-sm"
                    style={{ background: categoryVar(e.category) }}
                  />
                  {e.ext ? `.${e.ext}` : t("types.noExtension")}
                </td>
                <td className="tabular text-right text-ink-3">
                  {t("common.files", { count: e.files, formatted: formatCount(e.files) })}
                </td>
                <td className="tabular w-20 text-right">{formatBytes(value(e))}</td>
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}
