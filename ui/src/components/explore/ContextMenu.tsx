import { ClipboardCopy, FolderOpen, ZoomIn } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { MenuTarget } from "./TreemapView";

export interface MenuAction {
  id: "openInExplorer" | "copyPath" | "zoomInto";
  run: () => void;
}

/**
 * Right-click menu for items in Explore. Deliberately offers only safe actions: show the item in
 * File Explorer, copy its path, or zoom into a folder. Removing anything is only ever possible
 * through the Cleanup review flow (SAFETY.md).
 */
export function ContextMenu({
  target,
  x,
  y,
  actions,
  onClose,
}: {
  target: MenuTarget;
  x: number;
  y: number;
  actions: MenuAction[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.min(x, window.innerWidth - r.width - 8),
      top: Math.min(y, window.innerHeight - r.height - 8),
    });
    el.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
  }, [x, y]);

  useEffect(() => {
    const away = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  const icons = { openInExplorer: FolderOpen, copyPath: ClipboardCopy, zoomInto: ZoomIn };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t("menu.label", { name: target.name })}
      className="fade-in fixed z-50 min-w-[220px] rounded-lg border border-line bg-raised p-1 shadow-card"
      style={pos}
      onKeyDown={(e) => {
        const items = [...(ref.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === "Escape" || e.key === "Tab") {
          e.preventDefault();
          onClose();
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          items[(i + 1) % items.length]?.focus();
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          items[(i - 1 + items.length) % items.length]?.focus();
        }
      }}
    >
      {actions.map((a) => {
        const Icon = icons[a.id];
        return (
          <button
            key={a.id}
            type="button"
            role="menuitem"
            className="flex h-8 w-full items-center gap-2.5 rounded px-2.5 text-left text-[13px] hover:bg-hover focus:bg-hover focus:outline-none"
            onClick={() => {
              onClose();
              a.run();
            }}
          >
            <Icon size={16} aria-hidden className="text-ink-2" />
            {t(`menu.${a.id}`)}
          </button>
        );
      })}
    </div>
  );
}
