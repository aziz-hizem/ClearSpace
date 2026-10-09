import { describe, expect, it } from "vitest";
import { ageBucket, driveName, formatBytes, formatPercent } from "./format";

describe("formatting", () => {
  it("formats sizes like File Explorer (1024-based)", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(1)).toBe("1 byte");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatBytes(476 * 1024 ** 3)).toBe("476 GB");
    expect(formatBytes(-1)).toBe("–");
  });

  it("formats shares", () => {
    expect(formatPercent(0.5)).toBe("50%");
    expect(formatPercent(0.013)).toBe("1.3%");
  });

  it("buckets ages from newest to oldest", () => {
    const now = 30_000_000;
    expect(ageBucket(now - 10 * 1440, now)).toBe(0);
    expect(ageBucket(now - 100 * 1440, now)).toBe(1);
    expect(ageBucket(now - 300 * 1440, now)).toBe(2);
    expect(ageBucket(now - 700 * 1440, now)).toBe(3);
    expect(ageBucket(now - 2000 * 1440, now)).toBe(4);
    expect(ageBucket(0, now)).toBe(4);
  });

  it("names drives", () => {
    expect(driveName("C:\\", "Local Disk")).toBe("Local Disk (C:)");
    expect(driveName("D:\\", "")).toBe("D:");
  });
});
