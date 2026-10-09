/** Decodes the binary treemap layout produced by crates/treemap (`encode`). */

export const CELL_BYTES = 28;
const MAGIC = 0x4d545343; // "CSTM" little-endian

export const CellFlag = {
  Dir: 1 << 0,
  Header: 1 << 1,
  Cloud: 1 << 2,
  HiddenOrSystem: 1 << 3,
  Expanded: 1 << 4,
} as const;

/** Category index order matches clearspace_model::filetype::Category. */
export const CATEGORY_KEYS = [
  "documents",
  "images",
  "videos",
  "audio",
  "archives",
  "apps",
  "code",
  "data",
  "other",
] as const;

export interface TreemapCells {
  count: number;
  x: Float32Array;
  y: Float32Array;
  w: Float32Array;
  h: Float32Array;
  node: Uint32Array;
  depth: Uint8Array;
  flags: Uint8Array;
  /** 0..8 for files, 255 for folders. */
  category: Uint8Array;
  mtime: Uint32Array;
}

export function decodeTreemap(buf: ArrayBuffer): TreemapCells {
  const view = new DataView(buf);
  if (buf.byteLength < 8 || view.getUint32(0, true) !== MAGIC) throw new Error("Unexpected treemap data");
  const count = view.getUint32(4, true);
  if (buf.byteLength < 8 + count * CELL_BYTES) throw new Error("Truncated treemap data");
  const cells: TreemapCells = {
    count,
    x: new Float32Array(count),
    y: new Float32Array(count),
    w: new Float32Array(count),
    h: new Float32Array(count),
    node: new Uint32Array(count),
    depth: new Uint8Array(count),
    flags: new Uint8Array(count),
    category: new Uint8Array(count),
    mtime: new Uint32Array(count),
  };
  let o = 8;
  for (let i = 0; i < count; i++, o += CELL_BYTES) {
    cells.x[i] = view.getFloat32(o, true);
    cells.y[i] = view.getFloat32(o + 4, true);
    cells.w[i] = view.getFloat32(o + 8, true);
    cells.h[i] = view.getFloat32(o + 12, true);
    cells.node[i] = view.getUint32(o + 16, true);
    cells.depth[i] = view.getUint8(o + 20);
    cells.flags[i] = view.getUint8(o + 21);
    cells.category[i] = view.getUint8(o + 22);
    cells.mtime[i] = view.getUint32(o + 24, true);
  }
  return cells;
}

/** Index of the deepest cell containing (px, py), or -1. Cells are breadth-first, so scan backwards. */
export function hitTest(cells: TreemapCells, px: number, py: number): number {
  for (let i = cells.count - 1; i >= 0; i--) {
    const x = cells.x[i] ?? 0;
    const y = cells.y[i] ?? 0;
    if (px >= x && py >= y && px < x + (cells.w[i] ?? 0) && py < y + (cells.h[i] ?? 0)) return i;
  }
  return -1;
}
