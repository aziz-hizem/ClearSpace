import i18n from "../i18n";

const UNITS = ["bytes", "KB", "MB", "GB", "TB", "PB"] as const;

/** Sizes the way File Explorer shows them (1 KB = 1024 bytes). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "–";
  if (bytes < 1024) return i18n.t("units.bytes", { count: bytes });
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < UNITS.length - 1) {
    v /= 1024;
    u++;
  }
  const digits = v >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits, minimumFractionDigits: digits }).format(v)} ${UNITS[u]}`;
}

export function formatCount(n: number): string {
  return new Intl.NumberFormat(i18n.language).format(n);
}

export function formatPercent(fraction: number): string {
  if (!Number.isFinite(fraction)) return "–";
  const digits = fraction < 0.1 ? 1 : 0;
  return new Intl.NumberFormat(i18n.language, {
    style: "percent",
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(fraction);
}

export function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return i18n.t("units.seconds", { count: s });
  const m = Math.floor(s / 60);
  return i18n.t("units.minutesSeconds", { m, s: String(s % 60).padStart(2, "0") });
}

/** `mtime` is minutes since the Unix epoch (0 = unknown). */
export function formatDate(mtime: number): string {
  if (!mtime) return "–";
  return new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(new Date(mtime * 60_000));
}

export function formatRelative(date: Date, now = new Date()): string {
  const rtf = new Intl.RelativeTimeFormat(i18n.language, { numeric: "auto" });
  const diff = (date.getTime() - now.getTime()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 60) return rtf.format(Math.round(diff), "second");
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86_400) return rtf.format(Math.round(diff / 3600), "hour");
  return rtf.format(Math.round(diff / 86_400), "day");
}

/** Age bucket 0 (newest) … 4 (oldest) for colour-by-age. */
export function ageBucket(mtime: number, nowMinutes = Date.now() / 60_000): number {
  if (!mtime) return 4;
  const days = (nowMinutes - mtime) / 1440;
  if (days < 30) return 0;
  if (days < 182) return 1;
  if (days < 365) return 2;
  if (days < 3 * 365) return 3;
  return 4;
}

export function driveName(root: string, label: string): string {
  const letter = root.replace(/\\$/, "");
  return label ? `${label} (${letter})` : letter;
}
