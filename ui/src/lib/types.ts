// Mirrors crates/service/src/dto.rs (camelCase JSON). Keep in sync; the Rust tests pin the shape.

export type DriveKind = "fixed" | "removable" | "network" | "optical" | "ram" | "unknown";

export interface Drive {
  root: string;
  label: string;
  fileSystem: string;
  kind: DriveKind;
  totalBytes: number;
  freeBytes: number;
  ready: boolean;
}

export type ScanStatus =
  | "running"
  | "completed"
  | "cancelled"
  | "volumeLost"
  | "outOfMemory"
  | "rootUnavailable";

export interface ScanProgress {
  scanId: number;
  root: string;
  files: number;
  dirs: number;
  bytes: number;
  alloc: number;
  issues: number;
  elapsedMs: number;
  current: string | null;
  status: ScanStatus;
  message: string | null;
}

export type ScanEvent = { event: "progress"; data: ScanProgress } | { event: "finished"; data: ScanProgress };

export type Category =
  | "documents"
  | "images"
  | "videos"
  | "audio"
  | "archives"
  | "apps"
  | "code"
  | "data"
  | "other";

export interface NodeRow {
  id: number;
  name: string;
  isDir: boolean;
  isLink: boolean;
  size: number;
  alloc: number;
  files: number;
  /** Minutes since the Unix epoch; 0 = unknown. */
  mtime: number;
  childCount: number;
  cloud: boolean;
  hidden: boolean;
  unreadable: boolean;
  hardlink: boolean;
  category: Category | null;
}

export type SortKey = "size" | "allocated" | "name" | "files" | "modified";

export interface ChildrenRequest {
  node: number;
  sort: SortKey;
  descending: boolean;
  offset: number;
  limit: number;
}

export interface Children {
  total: number;
  offset: number;
  rows: NodeRow[];
}

export interface Crumb {
  id: number;
  name: string;
}

export interface NodeInfo {
  node: NodeRow;
  path: string;
  parent: number | null;
  ancestors: Crumb[];
}

export type Metric = "allocated" | "logical";

export interface TreemapRequest {
  node: number;
  width: number;
  height: number;
  metric: Metric;
  maxDepth: number;
}

export interface CategoryStat {
  category: Category;
  size: number;
  alloc: number;
  files: number;
}

export interface ExtStat {
  ext: string;
  category: Category;
  size: number;
  alloc: number;
  files: number;
}

export interface TypeBreakdown {
  categories: CategoryStat[];
  extensions: ExtStat[];
}

export interface Issue {
  node: number;
  path: string;
  reason: string;
  partial: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  logDir: string | null;
}

export interface BackendError {
  code: "unknownScan" | "unknownNode" | "badRequest" | "io";
  message: string;
}

/** Everything the UI can ask of the backend. Note: there is no delete, move or write here. */
export interface Backend {
  readonly isPreview: boolean;
  appInfo(): Promise<AppInfo>;
  listDrives(): Promise<Drive[]>;
  pickFolder(): Promise<string | null>;
  startScan(root: string, onEvent: (e: ScanEvent) => void): Promise<number>;
  cancelScan(scanId: number): Promise<void>;
  closeScan(scanId: number): Promise<void>;
  children(scanId: number, request: ChildrenRequest): Promise<Children>;
  rows(scanId: number, nodes: number[]): Promise<NodeRow[]>;
  nodeInfo(scanId: number, node: number): Promise<NodeInfo>;
  treemap(scanId: number, request: TreemapRequest): Promise<ArrayBuffer>;
  typeBreakdown(scanId: number, node: number): Promise<TypeBreakdown>;
  issues(scanId: number, limit: number): Promise<Issue[]>;
  openInExplorer(scanId: number, node: number): Promise<void>;
}
