import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend, ScanEvent } from "./types";

/** The real backend: Tauri IPC to crates/app (which wraps crates/service). */
export const tauriBackend: Backend = {
  isPreview: false,
  appInfo: () => invoke("app_info"),
  listDrives: () => invoke("list_drives"),
  async pickFolder() {
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  },
  startScan(root, onEvent) {
    const channel = new Channel<ScanEvent>();
    channel.onmessage = onEvent;
    return invoke("start_scan", { root, onEvent: channel });
  },
  cancelScan: (scanId) => invoke("cancel_scan", { scanId }),
  closeScan: (scanId) => invoke("close_scan", { scanId }),
  children: (scanId, request) => invoke("get_children", { scanId, request }),
  rows: (scanId, nodes) => invoke("get_rows", { scanId, nodes }),
  nodeInfo: (scanId, node) => invoke("get_node_info", { scanId, node }),
  treemap: (scanId, request) => invoke<ArrayBuffer>("get_treemap", { scanId, request }),
  typeBreakdown: (scanId, node) => invoke("get_type_breakdown", { scanId, node }),
  issues: (scanId, limit) => invoke("get_issues", { scanId, limit }),
  openInExplorer: (scanId, node) => invoke("open_in_explorer", { scanId, node }),
};
