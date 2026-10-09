import {
  Copy,
  Files,
  History,
  LayoutDashboard,
  Map as MapIcon,
  Settings,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { PRODUCT_NAME } from "../lib/product";
import { PAGES, type Page, useStore } from "../state/store";

const ICONS: Record<Page, typeof MapIcon> = {
  overview: LayoutDashboard,
  explore: MapIcon,
  cleanup: Sparkles,
  large: Files,
  duplicates: Copy,
  history: History,
  settings: Settings,
};

export function Sidebar() {
  const { t } = useTranslation();
  const page = useStore((s) => s.page);
  const go = useStore((s) => s.go);

  // Ctrl+1…7 switches pages.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.metaKey) return;
      const n = Number(e.key);
      const target = PAGES[n - 1];
      if (n >= 1 && target) {
        e.preventDefault();
        go(target);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const item = (p: Page, i: number) => {
    const Icon = ICONS[p];
    const active = p === page;
    return (
      <li key={p}>
        <button
          type="button"
          onClick={() => go(p)}
          aria-current={active ? "page" : undefined}
          title={`${t(`nav.${p}`)} (${t("nav.shortcut", { n: i + 1 })})`}
          className={`relative flex h-9 w-full items-center gap-3 rounded-md px-3 text-left transition-colors ${
            active ? "bg-hover font-semibold text-ink" : "text-ink-2 hover:bg-hover hover:text-ink"
          }`}
        >
          {active && (
            <span aria-hidden className="absolute top-2 bottom-2 left-0 w-[3px] rounded-full bg-accent" />
          )}
          <Icon size={18} aria-hidden className={active ? "text-accent-ink" : ""} />
          {t(`nav.${p}`)}
        </button>
      </li>
    );
  };

  return (
    <aside className="flex w-[232px] shrink-0 flex-col border-r border-line bg-surface">
      <div className="flex items-center gap-2.5 px-5 pt-5 pb-4">
        <img src="/icon.png" alt="" className="size-7" />
        <span className="text-[15px] font-semibold tracking-tight">{PRODUCT_NAME}</span>
      </div>
      <nav aria-label={t("nav.label")} className="flex flex-1 flex-col px-3">
        <ul className="flex flex-col gap-0.5">{PAGES.slice(0, 6).map(item)}</ul>
        <ul className="mt-auto mb-2 flex flex-col gap-0.5">{item("settings", 6)}</ul>
      </nav>
      <p className="flex items-start gap-2 border-t border-line px-5 py-3 text-xs text-ink-3">
        <ShieldCheck size={14} aria-hidden className="mt-0.5 shrink-0 text-success" />
        {t("app.offlineBadge")}
      </p>
    </aside>
  );
}
