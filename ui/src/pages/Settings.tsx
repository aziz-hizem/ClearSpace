import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Card, PageHeader, Segmented } from "../components/ui";
import { getBackend } from "../lib/backend";
import { PRODUCT_NAME } from "../lib/product";
import type { AppInfo } from "../lib/types";
import { useStore } from "../state/store";

export function Settings() {
  const { t } = useTranslation();
  const themePref = useStore((s) => s.themePref);
  const setThemePref = useStore((s) => s.setThemePref);
  const metric = useStore((s) => s.explore.metric);
  const setMetric = useStore((s) => s.setMetric);
  const colorBy = useStore((s) => s.explore.colorBy);
  const setColorBy = useStore((s) => s.setColorBy);
  const showOnboarding = useStore((s) => s.showOnboarding);
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    void getBackend()
      .then((b) => b.appInfo())
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);

  const row = (label: string, control: React.ReactNode) => (
    <div className="flex flex-wrap items-center justify-between gap-3 py-3">
      <span>{label}</span>
      {control}
    </div>
  );

  return (
    <div className="h-full overflow-y-auto">
      <PageHeader title={t("settings.title")} />
      <div className="flex max-w-2xl flex-col gap-5 px-8 pb-10">
        <Card className="px-5 py-2" aria-labelledby="s-appearance">
          <h2 id="s-appearance" className="pt-3 font-semibold">
            {t("settings.appearance")}
          </h2>
          {row(
            t("settings.theme"),
            <Segmented
              label={t("settings.theme")}
              value={themePref}
              onChange={setThemePref}
              options={[
                { value: "system", label: t("settings.themeSystem") },
                { value: "light", label: t("settings.themeLight") },
                { value: "dark", label: t("settings.themeDark") },
              ]}
            />,
          )}
        </Card>
        <Card className="px-5 py-2" aria-labelledby="s-map">
          <h2 id="s-map" className="pt-3 font-semibold">
            {t("settings.map")}
          </h2>
          {row(
            t("settings.defaultMetric"),
            <Segmented
              label={t("settings.defaultMetric")}
              value={metric}
              onChange={setMetric}
              options={[
                {
                  value: "allocated",
                  label: t("explore.metricAllocated"),
                  hint: t("explore.metricAllocatedHint"),
                },
                { value: "logical", label: t("explore.metricLogical"), hint: t("explore.metricLogicalHint") },
              ]}
            />,
          )}
          {row(
            t("settings.defaultColor"),
            <Segmented
              label={t("settings.defaultColor")}
              value={colorBy}
              onChange={setColorBy}
              options={[
                { value: "type", label: t("explore.colorType") },
                { value: "age", label: t("explore.colorAge") },
              ]}
            />,
          )}
        </Card>
        <Card className="px-5 py-4" aria-labelledby="s-privacy">
          <h2 id="s-privacy" className="font-semibold">
            {t("settings.privacy")}
          </h2>
          <p className="mt-2 text-ink-2">{t("settings.privacyBody", { name: PRODUCT_NAME })}</p>
        </Card>
        <Card className="px-5 py-4" aria-labelledby="s-about">
          <h2 id="s-about" className="font-semibold">
            {t("settings.about")}
          </h2>
          <p className="mt-2">
            {PRODUCT_NAME} ·{" "}
            <span className="text-ink-2">{t("settings.version", { version: info?.version ?? "…" })}</span>
          </p>
          {info?.logDir && (
            <p className="mt-1 text-[13px] break-all text-ink-3">
              {t("settings.logs", { path: info.logDir })}
            </p>
          )}
          <div className="mt-4">
            <Button onClick={showOnboarding}>{t("settings.showTour")}</Button>
          </div>
        </Card>
      </div>
    </div>
  );
}
