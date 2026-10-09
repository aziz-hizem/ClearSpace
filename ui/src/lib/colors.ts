import { CATEGORY_KEYS } from "./treemapData";
import type { Category } from "./types";

/**
 * Colours are CSS custom properties (styles.css) so light/dark themes stay in one place.
 * Categorical slots follow the validated reference palette in a FIXED category→slot order
 * (never re-assigned by rank). In the treemap any two colours can touch, so identity is never
 * colour-alone: the legend, the tooltip and the "File types" table always name the category.
 */
export interface Palette {
  category: string[]; // indexed like CATEGORY_KEYS
  age: string[]; // 0 = newest … 4 = oldest
  folder: string[];
  headerText: string;
  surface: string;
  sunken: string;
  ink: string;
  accent: string;
}

export function readPalette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim() || "#888888";
  return {
    category: CATEGORY_KEYS.map((k) => v(`--cat-${k}`)),
    age: [0, 1, 2, 3, 4].map((i) => v(`--age-${i}`)),
    folder: [v("--tm-folder-1"), v("--tm-folder-2"), v("--tm-folder-3")],
    headerText: v("--tm-header-text"),
    surface: v("--surface"),
    sunken: v("--surface-sunken"),
    ink: v("--text"),
    accent: v("--accent"),
  };
}

export const categoryVar = (c: Category) => `var(--cat-${c})`;

/** Black or white text, whichever reads better on `hex`. */
export function inkOn(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m?.[1]) return "#0b0b0b";
  const n = Number.parseInt(m[1], 16);
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  const l = 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
  return (l + 0.05) / 0.05 > 1.05 / (l + 0.05) ? "#0b0b0b" : "#ffffff";
}
