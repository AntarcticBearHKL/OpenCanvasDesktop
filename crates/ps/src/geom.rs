//! Small shared geometry primitives (points and axis-aligned boxes) used by the
//! transform, path, board and layer-op modules.

/// A 2D point in board or layer-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned box `{ x, y, width, height }` (the web's `PsBox`/`PsDocBox`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// `psBoxUnion` (smart-canvas.ts:133): the union of zero or more boxes.
    pub fn union(boxes: &[Bounds]) -> Option<Bounds> {
        let first = boxes.first()?;
        Some(boxes.iter().skip(1).fold(*first, |box_, item| {
            let x = box_.x.min(item.x);
            let y = box_.y.min(item.y);
            Bounds {
                x,
                y,
                width: (box_.x + box_.width).max(item.x + item.width) - x,
                height: (box_.y + box_.height).max(item.y + item.height) - y,
            }
        }))
    }
}

/// `psRotatePoint` (smart-canvas.ts:119): rotate `(x, y)` around `centre` by
/// `degrees`.
pub fn rotate_point(x: f64, y: f64, centre: Point, degrees: f64) -> Point {
    let angle = degrees.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let dx = x - centre.x;
    let dy = y - centre.y;
    Point::new(
        centre.x + dx * cos - dy * sin,
        centre.y + dx * sin + dy * cos,
    )
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn box_union_of_nothing_is_none() {
        assert!(Bounds::union(&[]).is_none());
    }

    #[test]
    fn box_union_covers_both_boxes() {
        let union = Bounds::union(&[
            Bounds::new(0.0, 0.0, 10.0, 10.0),
            Bounds::new(5.0, 5.0, 10.0, 10.0),
        ])
        .expect("union");
        assert_eq!(union, Bounds::new(0.0, 0.0, 15.0, 15.0));
    }

    #[test]
    fn rotate_ninety_degrees_swaps_axes() {
        let out = rotate_point(1.0, 0.0, Point::new(0.0, 0.0), 90.0);
        assert!(out.x.abs() < 1e-9);
        assert!((out.y - 1.0).abs() < 1e-9);
    }
}
