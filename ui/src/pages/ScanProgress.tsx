import { useTranslation } from "react-i18next";
import { Spinner } from "../components/ui";
import { formatBytes, formatCount, formatDuration } from "../lib/format";
import type { ScanProgress } from "../lib/types";

/** Live scan numbers. Announced politely to screen readers (not on every tick). */
export function ScanProgressPanel({ progress: p }: { progress: ScanProgress }) {
  const { t } = useTranslation();
  const stat = (label: string, value: string) => (
    <div>
      <dt className="text-xs text-ink-3">{label}</dt>
      <dd className="tabular text-[15px] font-semibold">{value}</dd>
    </div>
  );
  return (
    <div>
      <Spinner label={t("scan.running")} />
      <dl className="mt-3 grid grid-cols-4 gap-3">
        {stat(t("scan.files"), formatCount(p.files))}
        {stat(t("scan.folders"), formatCount(p.dirs))}
        {stat(t("scan.found"), formatBytes(p.bytes))}
        {stat(t("scan.elapsed"), formatDuration(p.elapsedMs))}
      </dl>
      {p.current && (
        <p className="mt-2 truncate text-xs text-ink-3" title={p.current}>
          {t("scan.current", { path: p.current })}
        </p>
      )}
    </div>
  );
}
