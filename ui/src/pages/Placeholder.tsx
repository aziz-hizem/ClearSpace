import { Copy, Files, History, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { EmptyState, PageHeader } from "../components/ui";
import { PRODUCT_NAME } from "../lib/product";

const PAGES = {
  cleanup: {
    icon: Sparkles,
    title: "placeholder.cleanupTitle",
    body: "placeholder.cleanupBody",
    nav: "nav.cleanup",
  },
  large: { icon: Files, title: "placeholder.largeTitle", body: "placeholder.largeBody", nav: "nav.large" },
  duplicates: {
    icon: Copy,
    title: "placeholder.duplicatesTitle",
    body: "placeholder.duplicatesBody",
    nav: "nav.duplicates",
  },
  history: {
    icon: History,
    title: "placeholder.historyTitle",
    body: "placeholder.historyBody",
    nav: "nav.history",
  },
} as const;

/** Pages that arrive in later milestones: explain what they will do. */
export function Placeholder({ page }: { page: keyof typeof PAGES }) {
  const { t } = useTranslation();
  const p = PAGES[page];
  const Icon = p.icon;
  return (
    <div className="h-full overflow-y-auto">
      <PageHeader title={t(p.nav)} />
      <EmptyState
        icon={<Icon size={28} aria-hidden />}
        title={t(p.title)}
        body={t(p.body, { name: PRODUCT_NAME })}
        badge={t("placeholder.soon")}
      />
    </div>
  );
}
