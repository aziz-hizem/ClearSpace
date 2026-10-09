import { describe, expect, it } from "vitest";
import { createMockBackend } from "./mockBackend";
import { CellFlag, decodeTreemap, hitTest } from "./treemapData";

async function finishedScan() {
  const b = createMockBackend();
  const done = new Promise<number>((resolve) => {
    void b.startScan("C:\\", (e) => e.event === "finished" && resolve(e.data.scanId));
  });
  return { b, scanId: await done };
}

describe("treemap data", () => {
  it("decodes a layout and hit-tests the deepest cell", async () => {
    const { b, scanId } = await finishedScan();
    const cells = decodeTreemap(
      await b.treemap(scanId, { node: 0, width: 800, height: 500, metric: "allocated", maxDepth: 6 }),
    );
    expect(cells.count).toBeGreaterThan(50);
    // Depth-1 cells tile the whole canvas.
    let area = 0;
    for (let i = 0; i < cells.count; i++)
      if (cells.depth[i] === 1) area += (cells.w[i] ?? 0) * (cells.h[i] ?? 0);
    expect(area).toBeCloseTo(800 * 500, -2);
    const i = hitTest(cells, 400, 250);
    expect(i).toBeGreaterThanOrEqual(0);
    expect(cells.flags[i] ?? 0 & CellFlag.Dir).toBeDefined();
    expect(hitTest(cells, -5, -5)).toBe(-1);
  }, 10_000);

  it("rejects malformed data", () => {
    expect(() => decodeTreemap(new ArrayBuffer(4))).toThrow();
    expect(() => decodeTreemap(new ArrayBuffer(16))).toThrow();
  });
});
