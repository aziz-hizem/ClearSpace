import { CircleAlert, Info, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useStore } from "../state/store";

export function Toasts() {
  const { t } = useTranslation();
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed right-5 bottom-5 z-40 flex w-[360px] flex-col gap-2"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.tone === "error" ? "alert" : "status"}
          className="fade-in pointer-events-auto flex items-start gap-3 rounded-lg border border-line bg-raised px-4 py-3 shadow-card"
        >
          {toast.tone === "error" ? (
            <CircleAlert size={18} aria-hidden className="mt-px shrink-0 text-danger" />
          ) : (
            <Info size={18} aria-hidden className="mt-px shrink-0 text-accent-ink" />
          )}
          <p className="flex-1 text-[13px]">{toast.text}</p>
          <button
            type="button"
            aria-label={t("common.close")}
            onClick={() => dismiss(toast.id)}
            className="text-ink-3 hover:text-ink"
          >
            <X size={16} aria-hidden />
          </button>
        </div>
      ))}
    </div>
  );
}
