import {
  Disc,
  FolderOpen,
  HardDrive,
  Map as MapIcon,
  MemoryStick,
  Network,
  RefreshCw,
  Square,
  Usb,
  Zap,
} from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Badge, Button, Card, Meter, PageHeader, Spinner } from "../components/ui";
import { getBackend } from "../lib/backend";
import {
  driveName,
  formatBytes,
  formatCount,
  formatDuration,
  formatPercent,
  formatRelative,
} from "../lib/format";
import type { Drive, DriveKind } from "../lib/types";
import { errorMessage, rootKey, type ScanRecord, useStore } from "../state/store";
import { ScanProgressPanel } from "./ScanProgress";

const KIND_ICON: Record<DriveKind, typeof HardDrive> = {
  fixed: HardDrive,
  removable: Usb,
  network: Network,
  optical: Disc,
  ram: MemoryStick,
  unknown: HardDrive,
};

export function Overview() {
  const { t } = useTranslation();
  const drives = useStore((s) => s.drives);
  const drivesError = useStore((s) => s.drivesError);
  const loadDrives = useStore((s) => s.loadDrives);
  const scans = useStore((s) => s.scans);
  const startScan = useStore((s) => s.startScan);
  const toast = useStore((s) => s.toast);

  useEffect(() => {
    if (drives === null) void loadDrives();
  }, [drives, loadDrives]);

  const folderScans = Object.values(scans).filter(
    (r) => !drives?.some((d) => rootKey(d.root) === rootKey(r.root)),
  );

  const scanFolder = async () => {
    try {
      const folder = await (await getBackend()).pickFolder();
      if (folder) await startScan(folder);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader
        title={t("overview.title")}
        subtitle={t("overview.subtitle")}
        actions={
          <>
            <Button
              variant="subtle"
              icon={<RefreshCw size={16} aria-hidden />}
              onClick={() => void loadDrives()}
            >
              {t("overview.refresh")}
            </Button>
            <Button icon={<FolderOpen size={16} aria-hidden />} onClick={() => void scanFolder()}>
              {t("overview.scanFolder")}
            </Button>
          </>
        }
      />
      <div className="px-8 pb-10">
        {drives === null && <Spinner label={t("overview.loading")} />}
        {drivesError && <p className="text-danger">{t("overview.drivesError", { message: drivesError })}</p>}
        {drives?.length === 0 && !drivesError && <p className="text-ink-2">{t("overview.noDrives")}</p>}
        <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
          {drives?.map((d) => (
            <DriveCard key={d.root} drive={d} record={scans[rootKey(d.root)]} />
          ))}
          {folderScans.map((r) => (
            <FolderCard key={r.root} record={r} />
          ))}
        </div>
        <div className="mt-8 flex max-w-3xl flex-col gap-2 text-[13px] text-ink-2">
          <p>{t("overview.readOnlyNote")}</p>
          <p className="flex items-center gap-2">
            <Zap size={14} aria-hidden className="shrink-0" />
            <span>
              <strong className="font-semibold">{t("overview.fastScan")}</strong> ·{" "}
              {t("overview.fastScanSoon")}
            </span>
          </p>
        </div>
      </div>
    </div>
  );
}

function DriveCard({ drive, record }: { drive: Drive; record: ScanRecord | undefined }) {
  const { t } = useTranslation();
  const Icon = KIND_ICON[drive.kind];
  const name = driveName(drive.root, drive.label || t(`overview.kind.${drive.kind}`));
  const used = Math.max(0, drive.totalBytes - drive.freeBytes);
  const nearlyFull = drive.totalBytes > 0 && drive.freeBytes / drive.totalBytes < 0.1;
  const headingId = `drive-${drive.root.replace(/[^A-Za-z0-9]/g, "")}`;

  return (
    <Card className="fade-in flex flex-col p-5" aria-labelledby={headingId}>
      <div className="flex items-start gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-sunken text-accent-ink">
          <Icon size={22} aria-hidden />
        </div>
        <div className="min-w-0 flex-1">
          <h2 id={headingId} className="truncate text-[15px] font-semibold">
            {name}
          </h2>
          <p className="text-[13px] text-ink-3">
            {t(`overview.kind.${drive.kind}`)}
            {drive.fileSystem && ` · ${drive.fileSystem}`}
          </p>
        </div>
      </div>
      {drive.ready ? (
        <div className="mt-4">
          <Meter
            value={used}
            max={drive.totalBytes}
            label={t("overview.meterLabel", { name })}
            tone={nearlyFull ? "warning" : "accent"}
          />
          <div className="mt-1.5 flex justify-between text-[13px] tabular">
            <span className="text-ink-2">
              {t("overview.freeOf", {
                free: formatBytes(drive.freeBytes),
                total: formatBytes(drive.totalBytes),
              })}
            </span>
            <span className="text-ink-3">
              {t("overview.usedLabel", { percent: formatPercent(used / Math.max(1, drive.totalBytes)) })}
            </span>
          </div>
        </div>
      ) : (
        <p className="mt-4 text-[13px] text-ink-3">{t("overview.notReady")}</p>
      )}
      {drive.ready && <ScanArea root={drive.root} record={record} network={drive.kind === "network"} />}
    </Card>
  );
}

function FolderCard({ record }: { record: ScanRecord }) {
  return (
    <Card className="fade-in flex flex-col p-5">
      <div className="flex items-start gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-sunken text-accent-ink">
          <FolderOpen size={22} aria-hidden />
        </div>
        <h2 className="min-w-0 flex-1 truncate pt-2 text-[15px] font-semibold" title={record.root}>
          {record.root}
        </h2>
      </div>
      <ScanArea root={record.root} record={record} network={false} />
    </Card>
  );
}

function ScanArea({
  root,
  record,
  network,
}: {
  root: string;
  record: ScanRecord | undefined;
  network: boolean;
}) {
  const { t } = useTranslation();
  const startScan = useStore((s) => s.startScan);
  const stopScan = useStore((s) => s.stopScan);
  const openExplore = useStore((s) => s.openExplore);

  if (!record) {
    return (
      <div className="mt-5 flex items-center justify-between gap-3 border-t border-line pt-4">
        <span className="text-[13px] text-ink-3">
          {network ? t("overview.networkHint") : t("overview.notScanned")}
        </span>
        <Button variant="primary" onClick={() => void startScan(root)}>
          {t("overview.scan")}
        </Button>
      </div>
    );
  }
  const p = record.progress;
  if (p.status === "running") {
    return (
      <div className="mt-5 border-t border-line pt-4">
        <ScanProgressPanel progress={p} />
        <div className="mt-3 flex justify-end gap-2">
          <Button
            variant="subtle"
            icon={<MapIcon size={16} aria-hidden />}
            onClick={() => openExplore(record.scanId)}
          >
            {t("overview.explore")}
          </Button>
          <Button
            icon={<Square size={14} aria-hidden />}
            disabled={record.stopping}
            onClick={() => void stopScan(root)}
          >
            {record.stopping ? t("scan.stopping") : t("scan.stop")}
          </Button>
        </div>
      </div>
    );
  }
  const failed = p.status === "rootUnavailable";
  return (
    <div className="mt-5 border-t border-line pt-4">
      <p className={`text-[13px] ${p.status === "completed" ? "text-ink-2" : "text-warning"}`} role="status">
        {p.status === "completed"
          ? t("overview.scannedSummary", {
              when: formatRelative(new Date(record.finishedAt ?? Date.now())),
              files: t("common.files", { count: p.files, formatted: formatCount(p.files) }),
              duration: formatDuration(p.elapsedMs),
            })
          : (p.message ?? t(`scan.status.${p.status}`))}
      </p>
      {p.issues > 0 && (
        <p className="mt-1">
          <Badge tone="warning">{t("scan.issues", { count: p.issues })}</Badge>
        </p>
      )}
      {!failed && <p className="mt-2 text-[13px] text-ink-3">{t("overview.freeHeadlinePending")}</p>}
      <div className="mt-3 flex justify-end gap-2">
        <Button onClick={() => void startScan(root)}>{t("overview.scanAgain")}</Button>
        {!failed && (
          <Button
            variant="primary"
            icon={<MapIcon size={16} aria-hidden />}
            onClick={() => openExplore(record.scanId)}
          >
            {t("overview.explore")}
          </Button>
        )}
      </div>
    </div>
  );
}
