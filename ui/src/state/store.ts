import { create } from "zustand";
import { getBackend } from "../lib/backend";
import { load, oneOf, save } from "../lib/storage";
import type { Drive, Metric, ScanProgress } from "../lib/types";

export type Page = "overview" | "explore" | "cleanup" | "large" | "duplicates" | "history" | "settings";
export const PAGES: Page[] = ["overview", "explore", "cleanup", "large", "duplicates", "history", "settings"];
export type ThemePref = "system" | "light" | "dark";
export type ColorBy = "type" | "age";

export interface ScanRecord {
  scanId: number;
  root: string;
  progress: ScanProgress;
  startedAt: number;
  finishedAt: number | null;
  stopping: boolean;
}

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

interface State {
  page: Page;
  go: (page: Page) => void;

  themePref: ThemePref;
  setThemePref: (t: ThemePref) => void;

  drives: Drive[] | null;
  drivesError: string | null;
  loadDrives: () => Promise<void>;

  /** Latest scan per root (key: upper-cased root). */
  scans: Record<string, ScanRecord>;
  startScan: (root: string) => Promise<void>;
  stopScan: (root: string) => Promise<void>;

  explore: { scanId: number | null; node: number; metric: Metric; colorBy: ColorBy };
  openExplore: (scanId: number, node?: number) => void;
  setExploreNode: (node: number) => void;
  setMetric: (m: Metric) => void;
  setColorBy: (c: ColorBy) => void;

  toasts: Toast[];
  toast: (text: string, tone?: Toast["tone"]) => void;
  dismissToast: (id: number) => void;

  onboardingDone: boolean;
  finishOnboarding: () => void;
  showOnboarding: () => void;
}

export const rootKey = (root: string) => root.toUpperCase().replace(/\\+$/, "");

let toastId = 1;

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

export const useStore = create<State>()((set, get) => ({
  page: "overview",
  go: (page) => set({ page }),

  themePref: load("theme", "system", oneOf("system", "light", "dark")),
  setThemePref: (themePref) => {
    save("theme", themePref);
    set({ themePref });
  },

  drives: null,
  drivesError: null,
  async loadDrives() {
    try {
      const drives = await (await getBackend()).listDrives();
      set({ drives, drivesError: null });
    } catch (e) {
      set({ drivesError: errorMessage(e), drives: [] });
    }
  },

  scans: {},
  async startScan(root) {
    const key = rootKey(root);
    const backend = await getBackend();
    const previous = get().scans[key];
    try {
      const scanId = await backend.startScan(root, (e) => {
        set((s) => {
          const rec = s.scans[key];
          if (!rec || rec.scanId !== e.data.scanId) return s;
          const finished = e.event === "finished";
          return {
            scans: {
              ...s.scans,
              [key]: {
                ...rec,
                progress: e.data,
                finishedAt: finished ? Date.now() : null,
                stopping: rec.stopping && !finished,
              },
            },
          };
        });
      });
      const progress: ScanProgress = {
        scanId,
        root,
        files: 0,
        dirs: 0,
        bytes: 0,
        alloc: 0,
        issues: 0,
        elapsedMs: 0,
        current: null,
        status: "running",
        message: null,
      };
      set((s) => ({
        scans: {
          ...s.scans,
          [key]: { scanId, root, progress, startedAt: Date.now(), finishedAt: null, stopping: false },
        },
      }));
      // Free the previous scan of the same location; Explore follows the new one.
      if (previous) {
        void backend.closeScan(previous.scanId);
        if (get().explore.scanId === previous.scanId) get().openExplore(scanId);
      }
    } catch (e) {
      get().toast(errorMessage(e), "error");
    }
  },
  async stopScan(root) {
    const rec = get().scans[rootKey(root)];
    if (!rec) return;
    set((s) => ({ scans: { ...s.scans, [rootKey(root)]: { ...rec, stopping: true } } }));
    await (await getBackend()).cancelScan(rec.scanId);
  },

  explore: {
    scanId: null,
    node: 0,
    metric: load("metric", "allocated", oneOf("allocated", "logical")),
    colorBy: load("colorBy", "type", oneOf("type", "age")),
  },
  openExplore: (scanId, node = 0) =>
    set((s) => ({ page: "explore", explore: { ...s.explore, scanId, node } })),
  setExploreNode: (node) => set((s) => ({ explore: { ...s.explore, node } })),
  setMetric: (metric) => {
    save("metric", metric);
    set((s) => ({ explore: { ...s.explore, metric } }));
  },
  setColorBy: (colorBy) => {
    save("colorBy", colorBy);
    set((s) => ({ explore: { ...s.explore, colorBy } }));
  },

  toasts: [],
  toast(text, tone = "info") {
    const id = toastId++;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, text, tone }] }));
    setTimeout(() => get().dismissToast(id), tone === "error" ? 8000 : 3500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  onboardingDone: load("onboardingDone", false, (v): v is boolean => typeof v === "boolean"),
  finishOnboarding: () => {
    save("onboardingDone", true);
    set({ onboardingDone: true });
  },
  showOnboarding: () => set({ onboardingDone: false }),
}));

/** The scan shown in Explore: the one chosen, or else the most recently started. */
export function useExploreScan(): ScanRecord | undefined {
  return useStore((s) => {
    const all = Object.values(s.scans);
    return (
      all.find((r) => r.scanId === s.explore.scanId) ??
      all.reduce<ScanRecord | undefined>((a, b) => (!a || b.startedAt > a.startedAt ? b : a), undefined)
    );
  });
}
