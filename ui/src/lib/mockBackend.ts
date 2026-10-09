/**
 * Preview backend with sample data, used only when the UI runs in a plain browser (UI
 * development, screenshots, tests). Never loaded inside the desktop app — see backend.ts.
 * The treemap layout mirrors crates/treemap so the binary decoding path is exercised.
 */
import { CATEGORY_KEYS, CELL_BYTES } from "./treemapData";
import type {
  Backend,
  Category,
  CategoryStat,
  Children,
  Drive,
  ExtStat,
  NodeRow,
  ScanEvent,
  ScanProgress,
  SortKey,
} from "./types";

interface MNode {
  id: number;
  parent: number;
  name: string;
  isDir: boolean;
  isLink: boolean;
  size: number;
  alloc: number;
  mtime: number;
  files: number;
  cloud: boolean;
  hidden: boolean;
  unreadable: boolean;
  children: number[];
}

const CATEGORY_OF: Record<string, Category> = {};
const EXT_GROUPS: [Category, string[]][] = [
  ["documents", ["pdf", "docx", "xlsx", "pptx", "txt", "md", "csv"]],
  ["images", ["jpg", "png", "heic", "gif", "psd", "webp"]],
  ["videos", ["mp4", "mov", "mkv", "avi"]],
  ["audio", ["mp3", "flac", "wav", "m4a"]],
  ["archives", ["zip", "7z", "iso", "msi", "cab", "esd"]],
  ["apps", ["exe", "dll", "sys"]],
  ["code", ["js", "ts", "py", "rs", "pdb"]],
  ["data", ["json", "log", "dat", "db", "tmp", "dmp", "etl", "cache", "ttf", "pak"]],
];
for (const [cat, exts] of EXT_GROUPS) for (const e of exts) CATEGORY_OF[e] = cat;

function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

function categoryOf(name: string): Category {
  return CATEGORY_OF[extOf(name)] ?? "other";
}

/** Small deterministic PRNG (mulberry32) so screenshots are stable. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const NOW_MIN = Math.floor(Date.UTC(2026, 9, 9, 12, 0) / 60_000);
const DAY = 1440;

class SampleDrive {
  nodes: MNode[] = [];
  private r = rng(42);

  constructor(rootName: string) {
    this.add(-1, rootName, true);
    this.build();
    this.aggregate(0);
  }

  private add(
    parent: number,
    name: string,
    isDir: boolean,
    size = 0,
    ageDays = 30,
    extra: Partial<MNode> = {},
  ) {
    const id = this.nodes.length;
    const alloc = isDir ? 0 : Math.ceil(size / 4096) * 4096;
    this.nodes.push({
      id,
      parent,
      name,
      isDir,
      isLink: false,
      size: isDir ? 0 : size,
      alloc,
      mtime: NOW_MIN - Math.floor(ageDays * DAY),
      files: 0,
      cloud: false,
      hidden: false,
      unreadable: false,
      children: [],
      ...extra,
    });
    if (parent >= 0) this.nodes[parent]?.children.push(id);
    return id;
  }

  private dir(parent: number, name: string, extra: Partial<MNode> = {}) {
    return this.add(parent, name, true, 0, 60, extra);
  }

  private files(
    parent: number,
    count: number,
    name: (i: number) => string,
    size: () => number,
    age: () => number,
  ) {
    for (let i = 0; i < count; i++) this.add(parent, name(i), false, Math.floor(size()), age());
  }

  private between(lo: number, hi: number) {
    return lo + (hi - lo) * this.r();
  }

  private logBetween(lo: number, hi: number) {
    return Math.exp(Math.log(lo) + (Math.log(hi) - Math.log(lo)) * this.r());
  }

  private build() {
    const MB = 1024 * 1024;
    const GB = 1024 * MB;
    const b = (lo: number, hi: number) => () => this.logBetween(lo, hi);
    const age = (lo: number, hi: number) => () => this.between(lo, hi);

    const users = this.dir(0, "Users");
    const me = this.dir(users, "Alex");
    const docs = this.dir(me, "Documents");
    this.files(
      docs,
      140,
      (i) => `Report ${2019 + (i % 7)}-${i}.${["pdf", "docx", "xlsx", "pptx"][i % 4]}`,
      b(40e3, 30 * MB),
      age(5, 2200),
    );
    const taxes = this.dir(docs, "Taxes");
    this.files(taxes, 24, (i) => `Return ${2016 + (i % 9)}.pdf`, b(200e3, 4 * MB), age(300, 3000));
    const pics = this.dir(me, "Pictures");
    for (const year of [2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026]) {
      const y = this.dir(pics, String(year));
      this.files(
        y,
        220,
        (i) => `IMG_${year}${String(i).padStart(4, "0")}.${i % 9 === 0 ? "heic" : "jpg"}`,
        b(1.5 * MB, 9 * MB),
        age((2026 - year) * 365, (2026 - year) * 365 + 300),
      );
    }
    const vids = this.dir(me, "Videos");
    this.files(
      vids,
      18,
      (i) =>
        `${["Holiday", "Birthday", "Wedding", "Drone"][i % 4]} ${2018 + (i % 8)}.${i % 3 ? "mp4" : "mov"}`,
      b(400 * MB, 7 * GB),
      age(60, 2500),
    );
    const music = this.dir(me, "Music");
    this.files(
      music,
      260,
      (i) => `Track ${i + 1}.${i % 5 ? "mp3" : "flac"}`,
      b(3 * MB, 40 * MB),
      age(400, 3600),
    );
    const dl = this.dir(me, "Downloads");
    this.files(
      dl,
      60,
      (i) =>
        `${["setup", "installer", "driver", "update", "archive"][i % 5]}-${i}.${["exe", "msi", "zip", "iso", "7z"][i % 5]}`,
      b(2 * MB, 4 * GB),
      age(10, 1400),
    );
    const od = this.dir(me, "OneDrive");
    const odDocs = this.dir(od, "Shared documents");
    for (let i = 0; i < 40; i++)
      this.add(
        odDocs,
        `Team plan ${i}.pptx`,
        false,
        Math.floor(this.logBetween(2 * MB, 300 * MB)),
        this.between(20, 900),
        { cloud: true, alloc: 0 },
      );
    const appdata = this.dir(me, "AppData", { hidden: true });
    const local = this.dir(appdata, "Local");
    const temp = this.dir(local, "Temp");
    this.files(
      temp,
      420,
      (i) => `tmp${(i * 2654435761).toString(16).slice(0, 6)}.${["tmp", "log", "dat"][i % 3]}`,
      b(4e3, 60 * MB),
      age(1, 400),
    );
    const chrome = this.dir(this.dir(this.dir(local, "Google"), "Chrome"), "User Data");
    const cache = this.dir(this.dir(chrome, "Default"), "Cache");
    this.files(cache, 900, (i) => `f_${String(i).padStart(6, "0")}`, b(2e3, 2 * MB), age(0, 60));
    const discord = this.dir(this.dir(appdata, "Roaming"), "discord");
    this.files(this.dir(discord, "Cache"), 300, (i) => `data_${i}`, b(5e3, 4 * MB), age(0, 30));
    const npm = this.dir(local, "npm-cache");
    this.files(npm, 500, (i) => `pkg-${i}.tgz`, b(5e3, 5 * MB), age(5, 700));
    const crash = this.dir(local, "CrashDumps");
    this.files(crash, 6, (i) => `app.exe.${1000 + i}.dmp`, b(50 * MB, 900 * MB), age(30, 600));
    this.dir(me, "Private", { unreadable: true });

    const pf = this.dir(0, "Program Files");
    for (const app of [
      "Adobe",
      "Microsoft Office",
      "Steam",
      "Mozilla Firefox",
      "Blender Foundation",
      "NVIDIA Corporation",
      "dotnet",
    ]) {
      const a = this.dir(pf, app);
      this.files(
        a,
        120,
        (i) => `${app.split(" ")[0]?.toLowerCase()}_${i}.${["dll", "exe", "dat", "pak"][i % 4]}`,
        b(50e3, 400 * MB),
        age(30, 900),
      );
    }
    const steamGames = this.dir(this.dir(pf, "Steam Library"), "common");
    for (const g of ["Space Racer", "Castle Siege", "Puzzle Garden"]) {
      const d = this.dir(steamGames, g);
      this.files(d, 14, (i) => `${g.replace(" ", "")}_${i}.pak`, b(80 * MB, 2.5 * GB), age(20, 500));
    }
    const pf86 = this.dir(0, "Program Files (x86)");
    this.files(this.dir(pf86, "Common Files"), 200, (i) => `lib${i}.dll`, b(20e3, 30 * MB), age(100, 2000));

    const win = this.dir(0, "Windows", { hidden: false });
    this.files(
      this.dir(win, "System32"),
      1500,
      (i) => `module${i}.${i % 6 ? "dll" : "exe"}`,
      b(10e3, 40 * MB),
      age(30, 700),
    );
    this.files(this.dir(win, "WinSxS"), 1200, (i) => `component_${i}.dll`, b(10e3, 25 * MB), age(30, 900));
    this.files(
      this.dir(win, "Installer"),
      60,
      (i) => `${(i * 9973).toString(16)}.msi`,
      b(5 * MB, 600 * MB),
      age(60, 1500),
    );
    const sd = this.dir(this.dir(win, "SoftwareDistribution"), "Download");
    this.files(sd, 80, (i) => `update_${i}.cab`, b(1 * MB, 400 * MB), age(5, 200));
    this.files(this.dir(win, "Fonts"), 300, (i) => `font${i}.ttf`, b(50e3, 30 * MB), age(400, 2000));
    this.files(
      this.dir(0, "ProgramData"),
      400,
      (i) => `data${i}.${["db", "json", "log", "etl"][i % 4]}`,
      b(1e3, 80 * MB),
      age(1, 900),
    );
    this.add(0, "hiberfil.sys", false, 6.4 * GB, 0, { hidden: true });
    this.add(0, "pagefile.sys", false, 9.5 * GB, 0, { hidden: true });
    this.dir(0, "$Recycle.Bin", { hidden: true });
    this.add(0, "Documents and Settings", true, 0, 2000, { isLink: true, hidden: true });
  }

  private aggregate(id: number): MNode {
    const n = this.nodes[id] as MNode;
    if (!n.isDir) return n;
    n.size = 0;
    n.alloc = 0;
    n.files = 0;
    let newest = 0;
    for (const c of n.children) {
      const k = this.aggregate(c);
      if (k.isLink) continue;
      n.size += k.size;
      n.alloc += k.alloc;
      n.files += k.isDir ? k.files : 1;
      newest = Math.max(newest, k.mtime);
    }
    if (newest) n.mtime = newest;
    return n;
  }

  row(id: number): NodeRow {
    const n = this.nodes[id] as MNode;
    return {
      id,
      name: n.name,
      isDir: n.isDir,
      isLink: n.isLink,
      size: n.size,
      alloc: n.alloc,
      files: n.files,
      mtime: n.mtime,
      childCount: n.children.length,
      cloud: n.cloud,
      hidden: n.hidden,
      unreadable: n.unreadable,
      hardlink: false,
      category: n.isDir ? null : categoryOf(n.name),
    };
  }

  path(id: number): string {
    const parts: string[] = [];
    for (let c: number = id; c >= 0; c = (this.nodes[c] as MNode).parent)
      parts.push((this.nodes[c] as MNode).name);
    parts.reverse();
    const [root, ...rest] = parts;
    return (root ?? "") + rest.join("\\");
  }
}

function sortRows(rows: NodeRow[], sort: SortKey, desc: boolean) {
  const key: Record<SortKey, (r: NodeRow) => number | string> = {
    size: (r) => r.size,
    allocated: (r) => r.alloc,
    files: (r) => r.files,
    modified: (r) => r.mtime,
    name: (r) => r.name.toLowerCase(),
  };
  const k = key[sort];
  rows.sort((a, b) => {
    const x = k(a);
    const y = k(b);
    return x < y ? -1 : x > y ? 1 : 0;
  });
  if (desc) rows.reverse();
}

/** Mirrors crates/treemap::squarify. */
function squarify(areas: number[], bx: number, by: number, bw: number, bh: number) {
  const out: [number, number, number, number][] = [];
  let [x, y, w, h] = [bx, by, bw, bh];
  let i = 0;
  const worst = (max: number, min: number, sum: number, side: number) =>
    Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  while (i < areas.length) {
    const short = Math.min(w, h);
    if (short <= 0) {
      for (; i < areas.length; i++) out.push([x, y, 0, 0]);
      break;
    }
    let j = i + 1;
    let sum = areas[i] ?? 0;
    let wr = worst(sum, sum, sum, short);
    while (j < areas.length) {
      const a = areas[j] ?? 0;
      const next = worst(areas[i] ?? 0, a, sum + a, short);
      if (next > wr) break;
      wr = next;
      sum += a;
      j++;
    }
    const last = j === areas.length;
    if (w >= h) {
      const cw = last ? w : Math.min(sum / h, w);
      let cy = y;
      for (let k = i; k < j; k++) {
        const ih = k + 1 === j ? y + h - cy : (areas[k] ?? 0) / cw;
        out.push([x, cy, cw, Math.max(0, ih)]);
        cy += ih;
      }
      x += cw;
      w -= cw;
    } else {
      const rh = last ? h : Math.min(sum / w, h);
      let cx = x;
      for (let k = i; k < j; k++) {
        const iw = k + 1 === j ? x + w - cx : (areas[k] ?? 0) / rh;
        out.push([cx, y, Math.max(0, iw), rh]);
        cx += iw;
      }
      y += rh;
      h -= rh;
    }
    i = j;
  }
  return out;
}

function layoutTreemap(
  drive: SampleDrive,
  root: number,
  width: number,
  height: number,
  metric: "allocated" | "logical",
  maxDepth: number,
) {
  const cells: number[][] = [];
  const queue: [number, number, number, number, number, number][] = [[root, 0, 0, width, height, 0]];
  const val = (n: MNode) => (n.isLink ? 0 : metric === "allocated" ? n.alloc : n.size);
  while (queue.length) {
    const [id, x, y, w, h, depth] = queue.shift() as [number, number, number, number, number, number];
    if (depth >= maxDepth || cells.length >= 60_000) continue;
    const kids = (drive.nodes[id] as MNode).children
      .map((c) => drive.nodes[c] as MNode)
      .filter((k) => val(k) > 0)
      .sort((a, b) => val(b) - val(a));
    if (!kids.length) continue;
    const total = kids.reduce((s, k) => s + val(k), 0);
    const rects = squarify(
      kids.map((k) => (val(k) * w * h) / total),
      x,
      y,
      w,
      h,
    );
    for (let i = 0; i < kids.length; i++) {
      const k = kids[i] as MNode;
      const [rx, ry, rw, rh] = rects[i] as [number, number, number, number];
      if (rw * rh < 12 || rw < 1 || rh < 1) break;
      let flags = 0;
      if (k.cloud) flags |= 4;
      if (k.hidden) flags |= 8;
      let category = 255;
      if (k.isDir) {
        flags |= 1;
        const header = rw >= 48 && rh >= 40;
        const top = header ? 16 : 1;
        if (header) flags |= 2;
        const iw = rw - 2;
        const ih = rh - top - 1;
        if (iw >= 2 && ih >= 2 && depth + 1 < maxDepth) {
          flags |= 16;
          queue.push([k.id, rx + 1, ry + top, iw, ih, depth + 1]);
        }
      } else {
        category = CATEGORY_KEYS.indexOf(categoryOf(k.name));
      }
      cells.push([rx, ry, rw, rh, k.id, depth + 1, flags, category, k.mtime]);
    }
  }
  const buf = new ArrayBuffer(8 + cells.length * CELL_BYTES);
  const v = new DataView(buf);
  v.setUint32(0, 0x4d545343, true);
  v.setUint32(4, cells.length, true);
  cells.forEach((c, i) => {
    const o = 8 + i * CELL_BYTES;
    for (let k = 0; k < 4; k++) v.setFloat32(o + k * 4, c[k] ?? 0, true);
    v.setUint32(o + 16, c[4] ?? 0, true);
    v.setUint8(o + 20, c[5] ?? 0);
    v.setUint8(o + 21, c[6] ?? 0);
    v.setUint8(o + 22, c[7] ?? 0);
    v.setUint32(o + 24, c[8] ?? 0, true);
  });
  return buf;
}

const GB = 1024 ** 3;

export function createMockBackend(): Backend {
  const drives: Drive[] = [
    {
      root: "C:\\",
      label: "Local Disk",
      fileSystem: "NTFS",
      kind: "fixed",
      totalBytes: 476 * GB,
      freeBytes: 61 * GB,
      ready: true,
    },
    {
      root: "D:\\",
      label: "Data",
      fileSystem: "NTFS",
      kind: "fixed",
      totalBytes: 931 * GB,
      freeBytes: 512 * GB,
      ready: true,
    },
    {
      root: "E:\\",
      label: "USB Stick",
      fileSystem: "exFAT",
      kind: "removable",
      totalBytes: 58 * GB,
      freeBytes: 41 * GB,
      ready: true,
    },
    { root: "F:\\", label: "", fileSystem: "", kind: "removable", totalBytes: 0, freeBytes: 0, ready: false },
  ];
  const scans = new Map<number, { drive: SampleDrive; progress: ScanProgress; cancelled: boolean }>();
  let nextId = 1;
  const get = (scanId: number) => {
    const s = scans.get(scanId);
    if (!s) throw { code: "unknownScan", message: "That scan is no longer available." };
    return s;
  };
  const node = (d: SampleDrive, id: number) => {
    if (!d.nodes[id]) throw { code: "unknownNode", message: "That item is no longer in this scan." };
    return d.nodes[id] as MNode;
  };

  return {
    isPreview: true,
    appInfo: async () => ({ name: "ClearSpace", version: "0.1.0 (preview)", logDir: null }),
    listDrives: async () => drives,
    pickFolder: async () => "C:\\Users\\Alex",
    async startScan(root: string, onEvent: (e: ScanEvent) => void) {
      const scanId = nextId++;
      const drive = new SampleDrive(root.endsWith("\\") ? root : `${root}\\`);
      const total = drive.nodes[0] as MNode;
      const totalDirs = drive.nodes.filter((n) => n.isDir).length;
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
      const entry = { drive, progress, cancelled: false };
      scans.set(scanId, entry);
      const started = performance.now();
      const duration = 2600;
      const tick = () => {
        const t = Math.min(1, (performance.now() - started) / duration);
        const ease = 1 - (1 - t) ** 2;
        Object.assign(progress, {
          files: Math.floor(total.files * ease),
          dirs: Math.floor(totalDirs * ease),
          bytes: Math.floor(total.size * ease),
          alloc: Math.floor(total.alloc * ease),
          issues: t > 0.6 ? 1 : 0,
          elapsedMs: Math.floor(performance.now() - started),
          current: drive.path(Math.floor(ease * (drive.nodes.length - 1))),
        });
        if (entry.cancelled) {
          progress.status = "cancelled";
          onEvent({ event: "finished", data: { ...progress } });
        } else if (t >= 1) {
          progress.status = "completed";
          progress.current = null;
          onEvent({ event: "finished", data: { ...progress } });
        } else {
          onEvent({ event: "progress", data: { ...progress } });
          setTimeout(tick, 150);
        }
      };
      setTimeout(tick, 150);
      return scanId;
    },
    async cancelScan(scanId) {
      get(scanId).cancelled = true;
    },
    async closeScan(scanId) {
      scans.delete(scanId);
    },
    async children(scanId, req): Promise<Children> {
      const { drive } = get(scanId);
      const rows = node(drive, req.node).children.map((c) => drive.row(c));
      sortRows(rows, req.sort, req.descending);
      const offset = Math.min(req.offset, rows.length);
      return { total: rows.length, offset, rows: rows.slice(offset, offset + Math.min(req.limit, 5000)) };
    },
    async rows(scanId, ids) {
      const { drive } = get(scanId);
      return ids.filter((i) => drive.nodes[i]).map((i) => drive.row(i));
    },
    async nodeInfo(scanId, id) {
      const { drive } = get(scanId);
      node(drive, id);
      const ancestors = [];
      for (let c = id; c >= 0; c = (drive.nodes[c] as MNode).parent)
        ancestors.push({ id: c, name: (drive.nodes[c] as MNode).name });
      ancestors.reverse();
      const p = (drive.nodes[id] as MNode).parent;
      return { node: drive.row(id), path: drive.path(id), parent: p >= 0 ? p : null, ancestors };
    },
    async treemap(scanId, req) {
      const { drive } = get(scanId);
      node(drive, req.node);
      return layoutTreemap(drive, req.node, req.width, req.height, req.metric, req.maxDepth);
    },
    async typeBreakdown(scanId, id) {
      const { drive } = get(scanId);
      const perExt = new Map<string, ExtStat>();
      const stack = [id];
      while (stack.length) {
        const n = node(drive, stack.pop() as number);
        if (n.isDir) {
          if (!n.isLink) stack.push(...n.children);
          continue;
        }
        const ext = extOf(n.name);
        const e = perExt.get(ext) ?? { ext, category: categoryOf(n.name), size: 0, alloc: 0, files: 0 };
        e.size += n.size;
        e.alloc += n.alloc;
        e.files += 1;
        perExt.set(ext, e);
      }
      const cats = new Map<Category, CategoryStat>();
      for (const e of perExt.values()) {
        const c = cats.get(e.category) ?? { category: e.category, size: 0, alloc: 0, files: 0 };
        c.size += e.size;
        c.alloc += e.alloc;
        c.files += e.files;
        cats.set(e.category, c);
      }
      return {
        categories: [...cats.values()].sort((a, b) => b.alloc - a.alloc),
        extensions: [...perExt.values()].sort((a, b) => b.alloc - a.alloc).slice(0, 100),
      };
    },
    async issues(scanId) {
      const { drive } = get(scanId);
      const priv = drive.nodes.find((n) => n.unreadable);
      return priv
        ? [
            {
              node: priv.id,
              path: drive.path(priv.id),
              reason: "Windows doesn't allow this folder to be read",
              partial: false,
            },
          ]
        : [];
    },
    async openInExplorer() {
      // Preview: nothing to open.
    },
  };
}
