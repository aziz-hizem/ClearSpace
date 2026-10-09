import type { Backend } from "./types";

/** True inside the desktop app (Tauri injects this global). */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let instance: Promise<Backend> | null = null;

/**
 * The backend for this environment. Outside the desktop app (a plain browser, used for UI
 * development and screenshots) a clearly labelled preview backend with sample data is used;
 * it is never loaded inside the app.
 */
export function getBackend(): Promise<Backend> {
  instance ??= inTauri
    ? import("./tauriBackend").then((m) => m.tauriBackend)
    : import("./mockBackend").then((m) => m.createMockBackend());
  return instance;
}

/** For tests. */
export function setBackend(b: Backend) {
  instance = Promise.resolve(b);
}
