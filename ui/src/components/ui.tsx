import { LoaderCircle } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "subtle";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-on-accent hover:bg-accent-hover border border-transparent shadow-sm",
  secondary: "bg-raised text-ink border border-line-strong hover:bg-hover",
  subtle: "bg-transparent text-ink border border-transparent hover:bg-hover",
};

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md"; icon?: ReactNode }) {
  const sz = size === "sm" ? "h-7 px-2.5 text-[13px] gap-1.5" : "h-8 px-3.5 gap-2";
  return (
    <button
      type="button"
      className={`inline-flex shrink-0 items-center justify-center rounded-md font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${sz} ${variants[variant]} ${className}`}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  children,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={`inline-flex size-8 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-hover hover:text-ink disabled:opacity-40 ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}

/** A small radio group that looks like a segmented control. */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string; hint?: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-md border border-line bg-sunken p-0.5"
    >
      {options.map((o) => {
        const checked = o.value === value;
        return (
          // biome-ignore lint/a11y/useSemanticElements: WAI-ARIA radio-group pattern (roving tabindex, arrow keys) styled as a segmented control
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={checked}
            title={o.hint}
            tabIndex={checked ? 0 : -1}
            onClick={() => onChange(o.value)}
            onKeyDown={(e) => {
              const i = options.findIndex((x) => x.value === value);
              const dir =
                e.key === "ArrowRight" || e.key === "ArrowDown"
                  ? 1
                  : e.key === "ArrowLeft" || e.key === "ArrowUp"
                    ? -1
                    : 0;
              if (!dir) return;
              e.preventDefault();
              const next = options[(i + dir + options.length) % options.length];
              if (next) onChange(next.value);
              const group = e.currentTarget.parentElement;
              requestAnimationFrame(() =>
                group?.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus(),
              );
            }}
            className={`h-7 rounded px-3 text-[13px] transition-colors ${
              checked ? "bg-raised font-semibold text-ink shadow-sm" : "text-ink-2 hover:text-ink"
            }`}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

export function Card({ className = "", children, ...rest }: React.HTMLAttributes<HTMLElement>) {
  return (
    <section className={`rounded-card border border-line bg-surface shadow-card ${className}`} {...rest}>
      {children}
    </section>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-4 px-8 pt-7 pb-5">
      <div className="min-w-0">
        <h1 className="text-[26px] leading-tight font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-1 text-ink-2">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

export function EmptyState({
  icon,
  title,
  body,
  children,
  badge,
}: {
  icon: ReactNode;
  title: string;
  body: string;
  children?: ReactNode;
  badge?: string;
}) {
  return (
    <div className="fade-in mx-auto flex max-w-md flex-col items-center px-6 py-16 text-center">
      <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-sunken text-accent-ink">
        {icon}
      </div>
      {badge && (
        <span className="mb-2 rounded-full border border-line px-2.5 py-0.5 text-xs font-medium text-ink-2">
          {badge}
        </span>
      )}
      <h2 className="text-lg font-semibold">{title}</h2>
      <p className="mt-2 text-ink-2">{body}</p>
      {children && <div className="mt-5 flex gap-2">{children}</div>}
    </div>
  );
}

export function Spinner({ label, size = 16 }: { label?: string; size?: number }) {
  return (
    <span role={label ? "status" : undefined} className="inline-flex items-center gap-2 text-ink-2">
      <LoaderCircle size={size} className="animate-spin motion-reduce:animate-none" aria-hidden />
      {label && <span>{label}</span>}
    </span>
  );
}

/** Accessible meter (role="meter") for used space. */
export function Meter({
  value,
  max,
  label,
  tone = "accent",
}: {
  value: number;
  max: number;
  label: string;
  tone?: "accent" | "warning";
}) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    // biome-ignore lint/a11y/useSemanticElements: <meter> can't be styled consistently (and themed) in WebView2; role="meter" with full aria-value* is equivalent
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={`${Math.round(pct)}%`}
      className="h-2 w-full overflow-hidden rounded-full bg-track"
    >
      <div
        className={`meter-fill h-full rounded-full transition-[width] duration-500 ${tone === "warning" ? "bg-warning" : "bg-accent"}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

export function Badge({
  children,
  title,
  tone = "neutral",
}: {
  children: ReactNode;
  title?: string;
  tone?: "neutral" | "warning";
}) {
  return (
    <span
      title={title}
      className={`inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-px text-[11px] font-medium ${
        tone === "warning" ? "bg-warning/15 text-warning" : "bg-sunken text-ink-2"
      }`}
    >
      {children}
    </span>
  );
}
