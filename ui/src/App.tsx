import { FlaskConical } from "lucide-react";
import { lazy, Suspense, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Onboarding } from "./components/Onboarding";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { Spinner } from "./components/ui";
import { getBackend } from "./lib/backend";
import { PRODUCT_NAME } from "./lib/product";
import { useAppliedTheme } from "./lib/theme";
import { Overview } from "./pages/Overview";
import { Placeholder } from "./pages/Placeholder";
import { Settings } from "./pages/Settings";
import { useStore } from "./state/store";

const Explore = lazy(() => import("./pages/Explore").then((m) => ({ default: m.Explore })));

export function App() {
  const { t } = useTranslation();
  const page = useStore((s) => s.page);
  const themePref = useStore((s) => s.themePref);
  const onboardingDone = useStore((s) => s.onboardingDone);
  const [preview, setPreview] = useState(false);
  useAppliedTheme(themePref);

  useEffect(() => {
    document.title = PRODUCT_NAME;
    void getBackend().then((b) => setPreview(b.isPreview));
  }, []);

  return (
    <div className="flex h-full flex-col">
      {preview && (
        <p className="flex items-center justify-center gap-2 bg-warning/15 py-1 text-xs font-medium text-warning">
          <FlaskConical size={14} aria-hidden /> {t("app.previewBanner")}
        </p>
      )}
      <div className="flex min-h-0 flex-1">
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-50 focus:rounded focus:bg-raised focus:px-3 focus:py-2"
        >
          {t("app.skipToContent")}
        </a>
        <Sidebar />
        <main id="main" className="min-w-0 flex-1" tabIndex={-1}>
          {page === "overview" && <Overview />}
          {page === "explore" && (
            <Suspense
              fallback={
                <div className="p-8">
                  <Spinner />
                </div>
              }
            >
              <Explore />
            </Suspense>
          )}
          {(page === "cleanup" || page === "large" || page === "duplicates" || page === "history") && (
            <Placeholder page={page} />
          )}
          {page === "settings" && <Settings />}
        </main>
      </div>
      <Toasts />
      {!onboardingDone && <Onboarding />}
    </div>
  );
}
