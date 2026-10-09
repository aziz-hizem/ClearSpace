import { useEffect, useSyncExternalStore } from "react";
import type { ThemePref } from "../state/store";

const query = () => window.matchMedia("(prefers-color-scheme: dark)");

function subscribe(cb: () => void) {
  const mq = query();
  mq.addEventListener("change", cb);
  return () => mq.removeEventListener("change", cb);
}

/** Resolves the user's theme preference (following Windows when set to "system") and applies it. */
export function useAppliedTheme(pref: ThemePref): "light" | "dark" {
  const systemDark = useSyncExternalStore(
    subscribe,
    () => query().matches,
    () => false,
  );
  const resolved = pref === "system" ? (systemDark ? "dark" : "light") : pref;
  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
  }, [resolved]);
  return resolved;
}
