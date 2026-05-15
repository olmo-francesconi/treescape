use crate::Rect;

#[derive(Clone, Copy, Debug, Default)]
struct FRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl FRect {
    fn short(&self) -> f64 {
        self.w.min(self.h)
    }
}

fn worst(row: &[f64], short: f64) -> f64 {
    if row.is_empty() {
        return f64::INFINITY;
    }
    let sum: f64 = row.iter().sum();
    if sum <= 0.0 || short <= 0.0 {
        return f64::INFINITY;
    }
    let max = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = row.iter().cloned().fold(f64::INFINITY, f64::min);
    let s2 = sum * sum;
    let w2 = short * short;
    (w2 * max / s2).max(s2 / (w2 * min))
}

/// Lays out one row of cells. Within a row, neighboring cells share their
/// integer boundary so they tile perfectly (no gaps, no overlaps). The last
/// cell absorbs any rounding drift by snapping to the row's outer edge.
fn layout_row(sizes: &[f64], indices: &[usize], rect: &mut FRect, out: &mut [Rect]) {
    let sum: f64 = sizes.iter().sum();
    if sum <= 0.0 {
        return;
    }
    if rect.w >= rect.h {
        // Row stacks vertically inside a vertical strip of width `width_f`.
        let width_f = sum / rect.h;
        let x0 = rect.x.round().max(0.0) as i32;
        let x1 = (rect.x + width_f).round().max(0.0) as i32;
        let y_max = (rect.y + rect.h).round().max(0.0) as i32;
        let mut cum = 0.0;
        let mut y_prev = rect.y.round().max(0.0) as i32;
        for (i, &s) in sizes.iter().enumerate() {
            cum += s;
            let y_next = if i + 1 == sizes.len() {
                y_max
            } else {
                (rect.y + cum / width_f).round().max(0.0) as i32
            };
            out[indices[i]] = Rect {
                x: x0 as u32,
                y: y_prev as u32,
                width: (x1 - x0).max(0) as u32,
                height: (y_next - y_prev).max(0) as u32,
            };
            y_prev = y_next;
        }
        rect.x += width_f;
        rect.w -= width_f;
    } else {
        // Row stretches horizontally inside a horizontal strip of height `height_f`.
        let height_f = sum / rect.w;
        let y0 = rect.y.round().max(0.0) as i32;
        let y1 = (rect.y + height_f).round().max(0.0) as i32;
        let x_max = (rect.x + rect.w).round().max(0.0) as i32;
        let mut cum = 0.0;
        let mut x_prev = rect.x.round().max(0.0) as i32;
        for (i, &s) in sizes.iter().enumerate() {
            cum += s;
            let x_next = if i + 1 == sizes.len() {
                x_max
            } else {
                (rect.x + cum / height_f).round().max(0.0) as i32
            };
            out[indices[i]] = Rect {
                x: x_prev as u32,
                y: y0 as u32,
                width: (x_next - x_prev).max(0) as u32,
                height: (y1 - y0).max(0) as u32,
            };
            x_prev = x_next;
        }
        rect.y += height_f;
        rect.h -= height_f;
    }
}

/// Squarified treemap layout (Bruls, Huijbregts, van Wijk).
/// Output rects perfectly tile `area` — neighbours share integer boundaries.
pub fn squarify(weights: &[u64], area: Rect) -> Vec<Rect> {
    squarify_with_labels(weights, &[], area)
}

/// Like [`squarify`], but the row-break decision also penalizes rows where
/// cells would be too narrow to fit their labels. `name_widths[i]` is the
/// number of terminal columns child `i` needs to render its full label
/// (icon + name + padding). Pass an empty slice to get pure squarified
/// behaviour (equivalent to [`squarify`]).
///
/// In a wide rect (`rect.w >= rect.h`) all cells in a row share the same
/// width — adding items only widens the strip, so truncation can only
/// improve and the label term is essentially inert. In a tall rect each
/// cell's width is proportional to its size, so adding items narrows the
/// already-placed cells; the label term causes the algorithm to break the
/// row earlier in that case, keeping each cell wide enough for its name.
pub fn squarify_with_labels(weights: &[u64], name_widths: &[usize], area: Rect) -> Vec<Rect> {
    if weights.is_empty() || area.width == 0 || area.height == 0 {
        return vec![Rect::default(); weights.len()];
    }
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return vec![Rect::default(); weights.len()];
    }

    let area_f = (area.width as f64) * (area.height as f64);
    let sizes: Vec<f64> = weights
        .iter()
        .map(|&w| (w as f64) / (total as f64) * area_f)
        .collect();

    let mut out = vec![Rect::default(); weights.len()];
    let mut rect = FRect {
        x: area.x as f64,
        y: area.y as f64,
        w: area.width as f64,
        h: area.height as f64,
    };

    let use_labels = name_widths.len() == weights.len();
    let mut row: Vec<usize> = Vec::new();
    let mut row_sizes: Vec<f64> = Vec::new();
    let mut row_name_widths: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < sizes.len() {
        let s = sizes[i];
        if row_sizes.is_empty() {
            row_sizes.push(s);
            row.push(i);
            if use_labels {
                row_name_widths.push(name_widths[i]);
            }
            i += 1;
            continue;
        }
        let cur = row_cost(&row_sizes, &row_name_widths, &rect, use_labels);
        row_sizes.push(s);
        if use_labels {
            row_name_widths.push(name_widths[i]);
        }
        let next = row_cost(&row_sizes, &row_name_widths, &rect, use_labels);
        if next <= cur {
            row.push(i);
            i += 1;
        } else {
            row_sizes.pop();
            if use_labels {
                row_name_widths.pop();
            }
            layout_row(&row_sizes, &row, &mut rect, &mut out);
            row.clear();
            row_sizes.clear();
            row_name_widths.clear();
        }
    }
    if !row_sizes.is_empty() {
        layout_row(&row_sizes, &row, &mut rect, &mut out);
    }

    out
}

/// Combined cost of a candidate row: worst aspect ratio plus a penalty for
/// the fraction of cells whose width can't fit their full label. λ = 2.0
/// means "100% truncated row" adds +2.0, which is comparable to the
/// aspect-ratio range we see in practice (1.0–5.0) — significant but not
/// overwhelming.
fn row_cost(row_sizes: &[f64], row_name_widths: &[usize], rect: &FRect, use_labels: bool) -> f64 {
    let aspect = worst(row_sizes, rect.short());
    if !use_labels || row_name_widths.is_empty() {
        return aspect;
    }
    let sum: f64 = row_sizes.iter().sum();
    if sum <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return aspect;
    }
    // Each cell's eventual width depends on the strip orientation that
    // layout_row will pick (based on `rect.w >= rect.h`).
    let widths: Vec<f64> = if rect.w >= rect.h {
        let strip_width = sum / rect.h;
        vec![strip_width; row_sizes.len()]
    } else {
        row_sizes.iter().map(|s| s * rect.w / sum).collect()
    };
    let truncations = widths
        .iter()
        .zip(row_name_widths.iter())
        .filter(|(w, nw)| **w < **nw as f64)
        .count();
    let rate = truncations as f64 / row_sizes.len() as f64;
    // λ controls how strongly truncated labels penalize a row. Aspect-ratio
    // `worst()` typically ranges 1.5–10 in practice; λ = 5.0 means a fully
    // truncated row adds +5, so the algorithm accepts noticeably more
    // elongated cells to keep names readable.
    const LAMBDA: f64 = 5.0;
    aspect + LAMBDA * rate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers_area_exactly(rects: &[Rect], area: Rect) -> bool {
        let total_area: u64 = rects.iter().map(|r| r.width as u64 * r.height as u64).sum();
        total_area == area.width as u64 * area.height as u64
    }

    fn no_overlaps(rects: &[Rect]) -> bool {
        for (i, a) in rects.iter().enumerate() {
            if a.width == 0 || a.height == 0 {
                continue;
            }
            for b in rects.iter().skip(i + 1) {
                if b.width == 0 || b.height == 0 {
                    continue;
                }
                let x_overlap = a.x < b.x + b.width && b.x < a.x + a.width;
                let y_overlap = a.y < b.y + b.height && b.y < a.y + a.height;
                if x_overlap && y_overlap {
                    return false;
                }
            }
        }
        true
    }

    fn all_inside(rects: &[Rect], area: Rect) -> bool {
        rects.iter().all(|r| {
            r.x >= area.x
                && r.y >= area.y
                && r.x + r.width <= area.x + area.width
                && r.y + r.height <= area.y + area.height
        })
    }

    #[test]
    fn empty_input_returns_empty() {
        let rects = squarify(&[], Rect::new(0, 0, 100, 100));
        assert!(rects.is_empty());
    }

    #[test]
    fn zero_area_returns_zero_rects_of_correct_count() {
        let rects = squarify(&[10, 20, 30], Rect::new(0, 0, 0, 100));
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.width == 0 && r.height == 0));
    }

    #[test]
    fn all_zero_weights_returns_zero_rects() {
        let rects = squarify(&[0, 0, 0], Rect::new(0, 0, 100, 50));
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.width == 0 && r.height == 0));
    }

    #[test]
    fn single_weight_fills_area() {
        let area = Rect::new(5, 10, 40, 20);
        let rects = squarify(&[42], area);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0], area);
    }

    #[test]
    fn perfect_tiling_simple() {
        let area = Rect::new(0, 0, 100, 50);
        let rects = squarify(&[10, 20, 30, 40], area);
        assert!(covers_area_exactly(&rects, area), "rects must tile area");
        assert!(no_overlaps(&rects), "rects must not overlap");
        assert!(all_inside(&rects, area), "rects must stay inside area");
    }

    #[test]
    fn perfect_tiling_many_random_inputs() {
        // Deterministic linear-congruential PRNG so the test is reproducible
        // without pulling in `rand`.
        let mut seed: u64 = 0x00C0_FFEE_BEEF;
        let mut rng = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            seed
        };

        for _ in 0..50 {
            let w = (rng() % 200 + 10) as u32;
            let h = (rng() % 100 + 10) as u32;
            let area = Rect::new(0, 0, w, h);
            let n = (rng() % 15 + 1) as usize;
            let weights: Vec<u64> = (0..n).map(|_| rng() % 1000 + 1).collect();

            let rects = squarify(&weights, area);
            assert_eq!(rects.len(), n);
            assert!(
                covers_area_exactly(&rects, area),
                "tiling broke for weights={weights:?} area={area:?} rects={rects:?}"
            );
            assert!(no_overlaps(&rects), "overlap for weights={weights:?}");
            assert!(
                all_inside(&rects, area),
                "out-of-bounds for weights={weights:?}"
            );
        }
    }
}
