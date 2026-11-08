//! Pearl's mark, [¶]: square brackets around a pilcrow, after MK Matrix's [aᵢⱼ]. In Typst, square
//! brackets hold content, and the pilcrow is the paragraph: a document's content, which is what
//! Pearl writes. Drawn in nacre, with a cream, rose and aqua sheen running across it; on a light
//! ground, in a deeper nacre that can still be read. Original work, drawn here in code: the same
//! shapes make the window's icon, pixel by pixel, and the mark the splash screen and the File view
//! paint.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Painter, Pos2, Rect, vec2};

/// The grid the shapes are given in, as MK Matrix's mark is: a square this many units wide.
const GRID: f32 = 512.0;
/// The mark is drawn a quarter larger around the grid's centre, as MK Matrix's is.
const SCALE: f32 = 1.25;

/// The stops of the sheen, from the top left to the bottom right: cream, rose, aqua.
const NACRE: [[f32; 3]; 3] = [[0.953, 0.925, 0.882], [0.918, 0.847, 0.831], [0.839, 0.890, 0.882]];
/// The same, deeper, for a light ground.
const DEEP_NACRE: [[f32; 3]; 3] = [[0.565, 0.522, 0.471], [0.580, 0.478, 0.502], [0.431, 0.522, 0.510]];

/// One of the mark's shapes, in grid units.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// A bar: left, top, right, bottom.
    Bar([f32; 4]),
    /// The left half of an ellipse whose centre is on its straight side: centre x, y, radii x, y.
    Bowl([f32; 4]),
}

impl Shape {
    fn contains(self, x: f32, y: f32) -> bool {
        match self {
            Shape::Bar([left, top, right, bottom]) => (left..=right).contains(&x) && (top..=bottom).contains(&y),
            Shape::Bowl([cx, cy, rx, ry]) => {
                let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
                x <= cx && dx * dx + dy * dy <= 1.0
            }
        }
    }
}

/// The shapes at a weight: the strokes' width in grid units. Heavier for small sizes, where the
/// pilcrow's stems would run together, so they also stand further apart.
fn shapes(weight: f32) -> [Shape; 10] {
    let h = weight / 2.0;
    // The brackets: the same as MK Matrix's.
    let (top, bottom, left, right, reach) = (112.0, 400.0, 108.0, 404.0, 44.0);
    // The pilcrow: a bar along the top, two stems down from it and the bowl at the left.
    let (stem, gap) = if weight > 30.0 { (264.0, 64.0) } else { (268.0, 50.0) };
    let (head, foot) = (137.0, 374.0);
    let bowl = [stem - h, head + 60.0, 72.0, 60.0];
    [
        Shape::Bar([left - h, top - h, left + reach, top + h]),
        Shape::Bar([left - h, top - h, left + h, bottom + h]),
        Shape::Bar([left - h, bottom - h, left + reach, bottom + h]),
        Shape::Bar([right - reach, top - h, right + h, top + h]),
        Shape::Bar([right - h, top - h, right + h, bottom + h]),
        Shape::Bar([right - reach, bottom - h, right + h, bottom + h]),
        Shape::Bar([stem - h, head, stem + gap + h, head + weight]),
        Shape::Bar([stem - h, head, stem + h, foot]),
        Shape::Bar([stem + gap - h, head, stem + gap + h, foot]),
        Shape::Bowl(bowl),
    ]
}

/// The weight for a mark this many pixels tall.
fn weight_for(side: f32) -> f32 {
    if side <= 28.0 { 36.0 } else { 26.0 }
}

/// Where a grid point is drawn, from 0 to 1 across the square.
fn place(x: f32, y: f32) -> (f32, f32) {
    let c = GRID / 2.0;
    (((x - c) * SCALE + c) / GRID, ((y - c) * SCALE + c) / GRID)
}

/// The grid point drawn at (`u`, `v`), from 0 to 1 across the square.
fn unplace(u: f32, v: f32) -> (f32, f32) {
    let c = GRID / 2.0;
    ((u * GRID - c) / SCALE + c, (v * GRID - c) / SCALE + c)
}

/// The sheen at a grid point.
fn sheen(stops: &[[f32; 3]; 3], x: f32, y: f32) -> [f32; 3] {
    let t = ((x - 90.0 + y - 90.0) / 660.0).clamp(0.0, 1.0);
    let (from, to, s) = if t < 0.5 { (0, 1, t * 2.0) } else { (1, 2, t * 2.0 - 1.0) };
    let (Some(a), Some(b)) = (stops.get(from), stops.get(to)) else { return [1.0; 3] };
    [0, 1, 2].map(|c| {
        let (p, q) = (a.get(c).copied().unwrap_or(1.0), b.get(c).copied().unwrap_or(1.0));
        p + (q - p) * s
    })
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn color(stops: &[[f32; 3]; 3], x: f32, y: f32) -> Color32 {
    let [r, g, b] = sheen(stops, x, y);
    Color32::from_rgb(to_byte(r), to_byte(g), to_byte(b))
}

/// Paints the mark in a square `side` points wide around `center`; `on_light` takes the deeper
/// nacre, for a light ground.
pub fn paint(painter: &Painter, center: Pos2, side: f32, on_light: bool) {
    let stops = if on_light { &DEEP_NACRE } else { &NACRE };
    let square = Rect::from_center_size(center, vec2(side, side));
    let at = |x: f32, y: f32| {
        let (u, v) = place(x, y);
        square.min + vec2(u, v) * side
    };
    let mut mesh = Mesh::default();
    let vertex = |mesh: &mut Mesh, x: f32, y: f32| -> u32 {
        let index = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex { pos: at(x, y), uv: egui::epaint::WHITE_UV, color: color(stops, x, y) });
        index
    };
    for shape in shapes(weight_for(side)) {
        match shape {
            // In pieces along its length, so the sheen runs through it.
            Shape::Bar([left, top, right, bottom]) => {
                const PIECES: u32 = 8;
                let along = bottom - top > right - left;
                for i in 0..PIECES {
                    let (s, e) = (i as f32 / PIECES as f32, (i + 1) as f32 / PIECES as f32);
                    let [l, t, r, b] = if along {
                        [left, top + (bottom - top) * s, right, top + (bottom - top) * e]
                    } else {
                        [left + (right - left) * s, top, left + (right - left) * e, bottom]
                    };
                    let corners = [vertex(&mut mesh, l, t), vertex(&mut mesh, r, t), vertex(&mut mesh, r, b), vertex(&mut mesh, l, b)];
                    let [a, b, c, d] = corners;
                    mesh.add_triangle(a, b, c);
                    mesh.add_triangle(a, c, d);
                }
            }
            Shape::Bowl([cx, cy, rx, ry]) => {
                const SEGMENTS: u32 = 40;
                let middle = vertex(&mut mesh, cx, cy);
                let mut last = None;
                for i in 0..=SEGMENTS {
                    let a = std::f32::consts::FRAC_PI_2 + i as f32 / SEGMENTS as f32 * std::f32::consts::PI;
                    let point = vertex(&mut mesh, cx + rx * a.cos(), cy - ry * a.sin());
                    if let Some(last) = last {
                        mesh.add_triangle(middle, last, point);
                    }
                    last = Some(point);
                }
            }
        }
    }
    painter.add(mesh);
}

/// The mark as a picture `side` pixels wide on a clear ground, each pixel covered as much as the
/// shapes cover it.
pub fn pixels(side: u32) -> Vec<u8> {
    /// Samples a pixel's side is cut into, for its edges' coverage.
    const SAMPLES: u32 = 4;
    let shapes = shapes(weight_for(side as f32));
    let side_f = side.max(1) as f32;
    let mut rgba = Vec::with_capacity((side as usize).saturating_mul(side as usize).saturating_mul(4));
    for py in 0..side {
        for px in 0..side {
            let mut covered = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / side_f;
                    let v = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / side_f;
                    let (x, y) = unplace(u, v);
                    if shapes.iter().any(|s| s.contains(x, y)) {
                        covered += 1;
                    }
                }
            }
            let (x, y) = unplace((px as f32 + 0.5) / side_f, (py as f32 + 0.5) / side_f);
            let [r, g, b, _] = color(&NACRE, x, y).to_array();
            let alpha = to_byte(covered as f32 / (SAMPLES * SAMPLES) as f32);
            rgba.extend_from_slice(&[r, g, b, alpha]);
        }
    }
    rgba
}

/// A grid point's place in pixels, for tests that look at a picture.
#[cfg(test)]
fn pixel_of(x: f32, y: f32, side: u32) -> (u32, u32) {
    let (u, v) = place(x, y);
    ((u * side as f32) as u32, (v * side as f32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(rgba: &[u8], side: u32, (x, y): (u32, u32)) -> u8 {
        rgba.get(((y * side + x) * 4 + 3) as usize).copied().unwrap_or(0)
    }

    #[test]
    fn the_picture_has_the_brackets_the_pilcrow_and_clear_ground() {
        for side in [16, 24, 32, 48, 128, 256] {
            let rgba = pixels(side);
            assert_eq!(rgba.len(), (side * side * 4) as usize);
            assert_eq!(alpha(&rgba, side, (0, 0)), 0, "{side}: a clear corner");
            assert_eq!(alpha(&rgba, side, pixel_of(170.0, 330.0, side)), 0, "{side}: clear under the bowl");
        }
        let side = 256;
        let rgba = pixels(side);
        // The left bracket's upright, the pilcrow's bowl, its stems and its bar.
        for (x, y) in [(108.0, 256.0), (220.0, 190.0), (268.0, 300.0), (318.0, 300.0), (293.0, 150.0)] {
            assert_eq!(alpha(&rgba, side, pixel_of(x, y, side)), 255, "({x}, {y}) is drawn");
        }
        // Between the stems, below the bar, the ground shows.
        assert_eq!(alpha(&rgba, side, pixel_of(293.0, 300.0, side)), 0);
    }

    #[test]
    fn the_sheen_runs_cream_rose_aqua() {
        let [r0, _, _] = sheen(&NACRE, 90.0, 90.0);
        let [r1, _, b1] = sheen(&NACRE, 420.0, 420.0);
        assert!(r0 > r1 && b1 > r1, "cream at the top left, aqua at the bottom right");
        for stops in [&NACRE, &DEEP_NACRE] {
            for i in -10..=60 {
                assert!(sheen(stops, i as f32 * 10.0, i as f32 * 10.0).iter().all(|v| (0.0..=1.0).contains(v)));
            }
        }
    }

    #[test]
    fn the_deep_nacre_reads_on_a_light_ground() {
        // Against Pearl's ivory, at least 3 to 1, as WCAG asks of a graphic.
        let luminance = |[r, g, b]: [f32; 3]| {
            let lin = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
        };
        let ivory = luminance([0.98, 0.97, 0.95]);
        for stop in DEEP_NACRE {
            assert!((ivory + 0.05) / (luminance(stop) + 0.05) >= 3.0, "{stop:?}");
        }
    }

    #[test]
    fn tiny_and_empty_pictures_are_whole() {
        assert!(pixels(0).is_empty());
        assert_eq!(pixels(1).len(), 4);
    }
}
