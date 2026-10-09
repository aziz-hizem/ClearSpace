import { Map as MapIcon, MessageSquareText, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PRODUCT_NAME } from "../lib/product";
import { useStore } from "../state/store";
import { Button } from "./ui";

const STEPS = [
  { icon: MapIcon, title: "onboarding.s1Title", body: "onboarding.s1Body" },
  { icon: MessageSquareText, title: "onboarding.s2Title", body: "onboarding.s2Body" },
  { icon: ShieldCheck, title: "onboarding.s3Title", body: "onboarding.s3Body" },
] as const;

/** First-run welcome: three short screens ending with the safety promise. */
export function Onboarding() {
  const { t } = useTranslation();
  const finish = useStore((s) => s.finishOnboarding);
  const [step, setStep] = useState(0);
  const dialogRef = useRef<HTMLDivElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const current = STEPS[step] ?? STEPS[0];
  const last = step === STEPS.length - 1;
  const Icon = current.icon;

  useEffect(() => {
    headingRef.current?.focus();
  }, []);

  // Keep Tab inside the dialog; Escape skips.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      finish();
      return;
    }
    if (e.key !== "Tab" || !dialogRef.current) return;
    const items = dialogRef.current.querySelectorAll<HTMLElement>("button:not([disabled])");
    const first = items[0];
    const lastItem = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      lastItem?.focus();
    } else if (!e.shiftKey && document.activeElement === lastItem) {
      e.preventDefault();
      first?.focus();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6 backdrop-blur-[2px]">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
        aria-describedby="onboarding-body"
        onKeyDown={onKeyDown}
        className="fade-in w-full max-w-[480px] rounded-xl border border-line bg-raised p-8 shadow-card"
      >
        <p className="text-xs font-medium text-ink-3">
          {t("onboarding.label", { name: PRODUCT_NAME })} ·{" "}
          {t("onboarding.step", { n: step + 1, total: STEPS.length })}
        </p>
        <div key={step} className="fade-in">
          <div className="mt-6 flex size-14 items-center justify-center rounded-2xl bg-sunken text-accent-ink">
            <Icon size={28} aria-hidden />
          </div>
          <h2
            id="onboarding-title"
            ref={headingRef}
            tabIndex={-1}
            className="mt-5 text-xl font-semibold outline-none"
          >
            {t(current.title)}
          </h2>
          <p id="onboarding-body" className="mt-2 min-h-[72px] text-ink-2" aria-live="polite">
            {t(current.body, { name: PRODUCT_NAME })}
          </p>
        </div>
        <div className="mt-6 flex items-center gap-1.5" aria-hidden>
          {STEPS.map((s, i) => (
            <span
              key={s.title}
              className={`h-1.5 rounded-full transition-all ${i === step ? "w-6 bg-accent" : "w-1.5 bg-line-strong"}`}
            />
          ))}
        </div>
        <div className="mt-6 flex justify-between gap-2">
          <Button variant="subtle" onClick={finish}>
            {t("onboarding.skip")}
          </Button>
          <div className="flex gap-2">
            {step > 0 && <Button onClick={() => setStep(step - 1)}>{t("common.back")}</Button>}
            <Button
              variant="primary"
              onClick={() => {
                if (last) finish();
                else {
                  setStep(step + 1);
                  headingRef.current?.focus();
                }
              }}
            >
              {last ? t("onboarding.start") : t("common.next")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
