import { ArrowUp, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Crumb } from "../../lib/types";
import { IconButton } from "../ui";

export function Breadcrumbs({ crumbs, onGo }: { crumbs: Crumb[]; onGo: (id: number) => void }) {
  const { t } = useTranslation();
  const parent = crumbs.length > 1 ? crumbs[crumbs.length - 2] : undefined;
  return (
    <div className="flex min-w-0 items-center gap-1">
      <IconButton label={t("explore.up")} disabled={!parent} onClick={() => parent && onGo(parent.id)}>
        <ArrowUp size={16} aria-hidden />
      </IconButton>
      <nav aria-label={t("explore.breadcrumbs")} className="min-w-0">
        <ol className="flex min-w-0 items-center">
          {crumbs.map((c, i) => {
            const last = i === crumbs.length - 1;
            return (
              <li key={c.id} className={`flex min-w-0 items-center ${last ? "" : "shrink"}`}>
                {i > 0 && <ChevronRight size={14} aria-hidden className="mx-0.5 shrink-0 text-ink-3" />}
                {last ? (
                  <span aria-current="location" className="truncate font-semibold">
                    {c.name}
                  </span>
                ) : (
                  <button
                    type="button"
                    className="truncate rounded px-1 text-ink-2 hover:bg-hover hover:text-ink"
                    onClick={() => onGo(c.id)}
                  >
                    {c.name}
                  </button>
                )}
              </li>
            );
          })}
        </ol>
      </nav>
    </div>
  );
}
