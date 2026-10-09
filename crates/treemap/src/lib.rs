//! Squarified treemap layout (Bruls, Huizing & van Wijk, 2000) for the Explore view.
//!
//! The layout is computed in Rust, close to the data, and shipped to the UI as a compact binary
//! buffer ([`encode`]). Only what can be seen is produced: recursion stops at a maximum depth,
//! below a minimum on-screen area, and after a total cell budget. Cells come out
//! breadth-first, so parents are painted before their children and a budget cut only ever
//! drops the deepest, smallest detail.

use std::collections::VecDeque;

use clearspace_model::filetype::category_of;
use clearspace_model::{NodeId, Tree, flags};

/// Which size drives the areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    /// Space actually used on disk. The default: cloud-only files take none.
    Allocated,
    /// Logical file size ("Size" in Explorer).
    Logical,
}

/// Layout parameters, in CSS pixels.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub width: f32,
    pub height: f32,
    pub metric: Metric,
    /// Levels below the root to lay out (1 = only the root's children).
    pub max_depth: u8,
    /// Cells smaller than this many square pixels are not emitted.
    pub min_area: f32,
    /// Hard cap on emitted cells.
    pub max_cells: usize,
    /// Height of a folder's title strip, when the folder is big enough to show one.
    pub header: f32,
    /// Gap between a folder's border and its contents.
    pub padding: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: 1000.0,
            height: 600.0,
            metric: Metric::Allocated,
            max_depth: 12,
            min_area: 12.0,
            max_cells: 60_000,
            header: 16.0,
            padding: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn area(&self) -> f32 {
        self.w * self.h
    }
}

/// Cell flag bits (see [`Cell::flags`]).
pub mod cell_flags {
    pub const DIR: u8 = 1 << 0;
    /// The cell shows a title strip with the folder name.
    pub const HEADER: u8 = 1 << 1;
    pub const CLOUD: u8 = 1 << 2;
    pub const HIDDEN_OR_SYSTEM: u8 = 1 << 3;
    /// A folder whose contents were laid out (otherwise it is drawn as one block).
    pub const EXPANDED: u8 = 1 << 4;
}

/// One rectangle of the treemap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub rect: Rect,
    pub node: NodeId,
    /// Depth below the layout root (children of the root are depth 1).
    pub depth: u8,
    pub flags: u8,
    /// `clearspace_model::filetype::Category` as `u8` (files only; folders use 255).
    pub category: u8,
    /// Last-write time in stored minutes (see `clearspace_model::time`).
    pub mtime: u32,
}

fn value(tree: &Tree, id: NodeId, metric: Metric) -> f64 {
    let n = tree.node(id);
    if n.has(flags::LINK) || n.has(flags::HARDLINK_DUP) {
        return 0.0;
    }
    (match metric {
        Metric::Allocated => n.alloc,
        Metric::Logical => n.size,
    }) as f64
}

/// Lays out the subtree under `root`. `root` itself is not emitted; its children fill the
/// whole canvas. The tree must be aggregated.
pub fn layout(tree: &Tree, root: NodeId, opts: &Options) -> Vec<Cell> {
    let mut cells = Vec::new();
    if opts.width < 1.0 || opts.height < 1.0 {
        return cells;
    }
    let mut ext_cat: Vec<u8> = (0..tree.exts().len())
        .map(|i| category_of(tree.exts().name(i as u16)) as u8)
        .collect();
    ext_cat.push(category_of("") as u8);
    let cat = |ext: u16| ext_cat[(ext as usize).min(ext_cat.len() - 1)];

    let canvas = Rect {
        x: 0.0,
        y: 0.0,
        w: opts.width,
        h: opts.height,
    };
    let mut queue: VecDeque<(NodeId, Rect, u8)> = VecDeque::new();
    queue.push_back((root, canvas, 0));
    let mut kids: Vec<(NodeId, f64)> = Vec::new();
    let mut rects: Vec<Rect> = Vec::new();

    while let Some((parent, area, depth)) = queue.pop_front() {
        if depth >= opts.max_depth || cells.len() >= opts.max_cells {
            continue;
        }
        kids.clear();
        kids.extend(
            tree.children(parent)
                .map(|c| (c, value(tree, c, opts.metric)))
                .filter(|&(_, v)| v > 0.0),
        );
        if kids.is_empty() {
            continue;
        }
        kids.sort_by(|a, b| b.1.total_cmp(&a.1));
        let total: f64 = kids.iter().map(|k| k.1).sum();
        let scale = f64::from(area.area()) / total;
        let areas: Vec<f64> = kids.iter().map(|k| k.1 * scale).collect();
        rects.clear();
        squarify(&areas, area, &mut rects);

        for (&(id, _), &r) in kids.iter().zip(rects.iter()) {
            if r.area() < opts.min_area || r.w < 1.0 || r.h < 1.0 {
                // Sorted by size: everything after this is smaller too.
                break;
            }
            if cells.len() >= opts.max_cells {
                break;
            }
            let n = tree.node(id);
            let mut f = 0u8;
            if n.has(flags::CLOUD) {
                f |= cell_flags::CLOUD;
            }
            if n.has(flags::HIDDEN) || n.has(flags::SYSTEM) {
                f |= cell_flags::HIDDEN_OR_SYSTEM;
            }
            let mut category = 255u8;
            if n.is_dir() {
                f |= cell_flags::DIR;
                let pad = opts.padding;
                let show_header = r.w >= 48.0 && r.h >= opts.header * 2.5;
                let top = if show_header { opts.header } else { pad };
                let inner = Rect {
                    x: r.x + pad,
                    y: r.y + top,
                    w: r.w - 2.0 * pad,
                    h: r.h - top - pad,
                };
                if show_header {
                    f |= cell_flags::HEADER;
                }
                if inner.w >= 2.0 && inner.h >= 2.0 && depth + 1 < opts.max_depth {
                    f |= cell_flags::EXPANDED;
                    queue.push_back((id, inner, depth + 1));
                }
            } else {
                category = cat(n.ext);
            }
            cells.push(Cell {
                rect: r,
                node: id,
                depth: depth + 1,
                flags: f,
                category,
                mtime: n.mtime,
            });
        }
    }
    cells
}

/// Squarified layout of `areas` (sorted descending, summing to `bounds.area()`) inside
/// `bounds`. Appends one rectangle per area, in order.
pub fn squarify(areas: &[f64], bounds: Rect, out: &mut Vec<Rect>) {
    let mut free = (
        f64::from(bounds.x),
        f64::from(bounds.y),
        f64::from(bounds.w),
        f64::from(bounds.h),
    );
    let mut i = 0;
    while i < areas.len() {
        let (x, y, w, h) = free;
        let short = w.min(h);
        if short <= 0.0 {
            // No room left (rounding): give the remaining items empty rectangles.
            out.extend(std::iter::repeat_n(
                Rect {
                    x: x as f32,
                    y: y as f32,
                    w: 0.0,
                    h: 0.0,
                },
                areas.len() - i,
            ));
            return;
        }
        // Grow the row while the worst aspect ratio improves.
        let mut j = i + 1;
        let mut sum = areas[i];
        let mut worst = worst_ratio(areas[i], areas[i], sum, short);
        while j < areas.len() {
            let next = worst_ratio(areas[i], areas[j], sum + areas[j], short);
            if next > worst {
                break;
            }
            worst = next;
            sum += areas[j];
            j += 1;
        }
        let last_row = j == areas.len();
        if w >= h {
            // Lay the row out as a column on the left.
            let col_w = if last_row { w } else { (sum / h).min(w) };
            let mut cy = y;
            for (k, &a) in areas[i..j].iter().enumerate() {
                let ih = if k + 1 == j - i {
                    y + h - cy
                } else {
                    a / col_w
                };
                out.push(Rect {
                    x: x as f32,
                    y: cy as f32,
                    w: col_w as f32,
                    h: ih.max(0.0) as f32,
                });
                cy += ih;
            }
            free = (x + col_w, y, w - col_w, h);
        } else {
            let row_h = if last_row { h } else { (sum / w).min(h) };
            let mut cx = x;
            for (k, &a) in areas[i..j].iter().enumerate() {
                let iw = if k + 1 == j - i {
                    x + w - cx
                } else {
                    a / row_h
                };
                out.push(Rect {
                    x: cx as f32,
                    y: y as f32,
                    w: iw.max(0.0) as f32,
                    h: row_h as f32,
                });
                cx += iw;
            }
            free = (x, y + row_h, w, h - row_h);
        }
        i = j;
    }
}

/// Worst aspect ratio of a row with largest item `max`, smallest `min`, total `sum`, laid
/// along a side of length `side`.
fn worst_ratio(max: f64, min: f64, sum: f64, side: f64) -> f64 {
    let s2 = sum * sum;
    let w2 = side * side;
    (w2 * max / s2).max(s2 / (w2 * min))
}

/// Bytes per encoded cell.
pub const CELL_BYTES: usize = 28;
/// Magic number at the start of an encoded layout ("CSTM").
pub const MAGIC: u32 = u32::from_le_bytes(*b"CSTM");

/// Encodes cells for the UI: an 8-byte header (`MAGIC`, cell count) followed by 28-byte
/// little-endian records `x, y, w, h: f32 · node: u32 · depth: u8 · flags: u8 · category: u8
/// · reserved: u8 · mtime: u32`.
pub fn encode(cells: &[Cell]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + cells.len() * CELL_BYTES);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&(cells.len() as u32).to_le_bytes());
    for c in cells {
        for v in [c.rect.x, c.rect.y, c.rect.w, c.rect.h] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&c.node.get().to_le_bytes());
        out.extend_from_slice(&[c.depth, c.flags, c.category, 0]);
        out.extend_from_slice(&c.mtime.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearspace_model::NewEntry;
    use std::ffi::OsStr;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn squarify_fills_bounds_exactly() {
        let bounds = Rect {
            x: 10.0,
            y: 20.0,
            w: 600.0,
            h: 400.0,
        };
        let areas = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let total: f64 = areas.iter().sum();
        let scaled: Vec<f64> = areas.iter().map(|a| a * 240_000.0 / total).collect();
        let mut out = Vec::new();
        squarify(&scaled, bounds, &mut out);
        assert_eq!(out.len(), areas.len());
        let sum: f32 = out.iter().map(Rect::area).sum();
        assert!((sum - 240_000.0).abs() < 1.0, "areas sum to bounds: {sum}");
        for (r, a) in out.iter().zip(&scaled) {
            assert!((r.area() as f64 - a).abs() / a < 0.001, "area preserved");
            assert!(r.x >= 10.0 - 0.01 && r.y >= 20.0 - 0.01);
            assert!(r.x + r.w <= 610.0 + 0.01 && r.y + r.h <= 420.0 + 0.01);
        }
        // No overlaps.
        for (i, a) in out.iter().enumerate() {
            for b in &out[i + 1..] {
                let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(ox <= 0.01 || oy <= 0.01, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn squarify_keeps_aspect_ratios_reasonable() {
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 1000.0,
            h: 600.0,
        };
        let raw: Vec<f64> = (1..=50).rev().map(f64::from).collect();
        let total: f64 = raw.iter().sum();
        let areas: Vec<f64> = raw.iter().map(|v| v * 600_000.0 / total).collect();
        let mut out = Vec::new();
        squarify(&areas, bounds, &mut out);
        let worst = out
            .iter()
            .map(|r| (r.w / r.h).max(r.h / r.w))
            .fold(0.0f32, f32::max);
        assert!(worst < 6.0, "worst aspect ratio {worst}");
    }

    #[test]
    fn single_item_takes_everything() {
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 300.0,
            h: 100.0,
        };
        let mut out = Vec::new();
        squarify(&[30_000.0], bounds, &mut out);
        assert!(approx(out[0].w, 300.0) && approx(out[0].h, 100.0));
    }

    fn sample_tree() -> Tree {
        let mut t = Tree::new(OsStr::new("R"));
        let d = |n| NewEntry {
            name: OsStr::new(n),
            flags: flags::DIR,
            size: 0,
            alloc: 0,
            mtime: 0,
        };
        let f = |n, s| NewEntry {
            name: OsStr::new(n),
            flags: 0,
            size: s,
            alloc: s,
            mtime: 5,
        };
        t.push_children(
            NodeId::ROOT,
            [d("big"), d("small"), f("movie.mp4", 3000), f("zero", 0)],
        )
        .unwrap();
        let big = t.find(&["big"]).unwrap();
        t.push_children(big, [f("a.jpg", 4000), f("b.dll", 2000)])
            .unwrap();
        let small = t.find(&["small"]).unwrap();
        let link = NewEntry {
            flags: flags::DIR | flags::LINK,
            ..d("link")
        };
        t.push_children(small, [f("c.txt", 1000), link]).unwrap();
        t.aggregate();
        t
    }

    #[test]
    fn nested_layout_breadth_first_and_inside_parents() {
        let t = sample_tree();
        let cells = layout(
            &t,
            NodeId::ROOT,
            &Options {
                width: 400.0,
                height: 300.0,
                ..Default::default()
            },
        );
        // big, movie, small at depth 1; zero-size file skipped; link skipped (0 bytes).
        let depth1: Vec<_> = cells
            .iter()
            .filter(|c| c.depth == 1)
            .map(|c| t.name(c.node).to_owned())
            .collect();
        assert_eq!(depth1, ["big", "movie.mp4", "small"]);
        assert!(
            cells.windows(2).all(|w| w[0].depth <= w[1].depth),
            "breadth-first order"
        );
        let total: f32 = cells
            .iter()
            .filter(|c| c.depth == 1)
            .map(|c| c.rect.area())
            .sum();
        assert!((total - 120_000.0).abs() < 1.0);
        let big = cells.iter().find(|c| t.name(c.node) == "big").unwrap();
        assert!(big.flags & cell_flags::DIR != 0 && big.flags & cell_flags::EXPANDED != 0);
        for c in cells
            .iter()
            .filter(|c| c.depth == 2 && t.parent(c.node) == t.find(&["big"]))
        {
            let (p, r) = (big.rect, c.rect);
            assert!(
                r.x >= p.x
                    && r.y >= p.y
                    && r.x + r.w <= p.x + p.w + 0.01
                    && r.y + r.h <= p.y + p.h + 0.01
            );
        }
        let jpg = cells.iter().find(|c| t.name(c.node) == "a.jpg").unwrap();
        assert_eq!(
            jpg.category,
            clearspace_model::filetype::Category::Images as u8
        );
        assert!(!cells.iter().any(|c| t.name(c.node) == "link"));
    }

    #[test]
    fn budgets_are_respected() {
        let t = sample_tree();
        let one_level = layout(
            &t,
            NodeId::ROOT,
            &Options {
                max_depth: 1,
                ..Default::default()
            },
        );
        assert!(one_level.iter().all(|c| c.depth == 1));
        let capped = layout(
            &t,
            NodeId::ROOT,
            &Options {
                max_cells: 2,
                ..Default::default()
            },
        );
        assert_eq!(capped.len(), 2);
        let tiny = layout(
            &t,
            NodeId::ROOT,
            &Options {
                width: 0.0,
                ..Default::default()
            },
        );
        assert!(tiny.is_empty());
    }

    #[test]
    fn encoding_round_trips() {
        let t = sample_tree();
        let cells = layout(&t, NodeId::ROOT, &Options::default());
        let bytes = encode(&cells);
        assert_eq!(bytes.len(), 8 + cells.len() * CELL_BYTES);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), MAGIC);
        let first = &bytes[8..8 + CELL_BYTES];
        assert_eq!(
            f32::from_le_bytes(first[8..12].try_into().unwrap()),
            cells[0].rect.w
        );
        assert_eq!(
            u32::from_le_bytes(first[16..20].try_into().unwrap()),
            cells[0].node.get()
        );
    }

    #[test]
    fn large_flat_directory_is_fast_and_capped() {
        let mut t = Tree::new(OsStr::new("R"));
        let names: Vec<String> = (0..200_000).map(|i| format!("f{i}.dat")).collect();
        let entries = names.iter().enumerate().map(|(i, n)| NewEntry {
            name: OsStr::new(n),
            flags: 0,
            size: (i % 1000 + 1) as u64,
            alloc: (i % 1000 + 1) as u64,
            mtime: 0,
        });
        t.push_children(NodeId::ROOT, entries.collect::<Vec<_>>())
            .unwrap();
        t.aggregate();
        let start = std::time::Instant::now();
        let cells = layout(&t, NodeId::ROOT, &Options::default());
        assert!(cells.len() <= 60_000);
        assert!(
            start.elapsed().as_secs_f32() < 2.0,
            "layout took {:?}",
            start.elapsed()
        );
    }
}
