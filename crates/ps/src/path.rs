//! Vector path geometry — port of `web/src/lib/canvas/ps-path.ts`: anchors,
//! cubic flattening, bounds, SVG path data and the arc-length sampler.

use crate::doc::{CanvasPsPath, CanvasPsPathAnchor};
use crate::geom::{Bounds, Point};

const CURVE_STEPS: usize = 24;

/// `psPathAnchor` (ps-path.ts:9).
pub fn path_anchor(x: f64, y: f64) -> CanvasPsPathAnchor {
    CanvasPsPathAnchor {
        x,
        y,
        handle_in: crate::doc::Position { x: 0.0, y: 0.0 },
        handle_out: crate::doc::Position { x: 0.0, y: 0.0 },
    }
}

/// `createPsPath` (ps-path.ts:13): the web mints a `nanoid`; the id is passed in.
pub fn create_path(
    id: &str,
    name: &str,
    anchors: Vec<CanvasPsPathAnchor>,
    closed: bool,
) -> CanvasPsPath {
    CanvasPsPath {
        id: id.to_string(),
        name: name.to_string(),
        anchors,
        closed,
        visible: true,
    }
}

/// `psSegment` (ps-path.ts:17): the four cubic control points of one segment.
pub fn segment(path: &CanvasPsPath, index: usize) -> Option<[Point; 4]> {
    let from = path.anchors.get(index)?;
    let to = match path.anchors.get(index + 1) {
        Some(next) => next,
        None if path.closed => path.anchors.first()?,
        None => return None,
    };
    Some([
        Point::new(from.x, from.y),
        Point::new(from.x + from.handle_out.x, from.y + from.handle_out.y),
        Point::new(to.x + to.handle_in.x, to.y + to.handle_in.y),
        Point::new(to.x, to.y),
    ])
}

/// `psCubicPoint` (ps-path.ts:30).
pub fn cubic_point(segment: &[Point; 4], t: f64) -> Point {
    let [p0, p1, p2, p3] = segment;
    let inverse = 1.0 - t;
    let (a, b, c, d) = (
        inverse * inverse * inverse,
        3.0 * inverse * inverse * t,
        3.0 * inverse * t * t,
        t * t * t,
    );
    Point::new(
        a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    )
}

/// `psPathSegmentCount` (ps-path.ts:38).
pub fn segment_count(path: &CanvasPsPath) -> usize {
    if path.closed {
        path.anchors.len()
    } else {
        path.anchors.len().saturating_sub(1)
    }
}

/// `psPathPolyline` (ps-path.ts:43).
pub fn polyline(path: &CanvasPsPath, steps: usize) -> Vec<Point> {
    let steps = steps.max(1);
    let mut points = Vec::new();
    for index in 0..segment_count(path) {
        let Some(seg) = segment(path, index) else {
            continue;
        };
        for step in 0..steps {
            points.push(cubic_point(&seg, step as f64 / steps as f64));
        }
    }
    let last = if path.closed {
        path.anchors.first()
    } else {
        path.anchors.last()
    };
    if let Some(anchor) = last {
        points.push(Point::new(anchor.x, anchor.y));
    }
    points
}

/// `psPathData` (ps-path.ts:56): `M ... L ...` polyline data.
pub fn path_data(path: &CanvasPsPath, offset: Point) -> String {
    let points: Vec<String> = polyline(path, CURVE_STEPS)
        .iter()
        .map(|point| format!("{:.2} {:.2}", point.x - offset.x, point.y - offset.y))
        .collect();
    if points.is_empty() {
        return String::new();
    }
    let close = if path.closed { " Z" } else { "" };
    format!("M {}{}", points.join(" L "), close)
}

/// `psPathBounds` (ps-path.ts:61).
pub fn path_bounds(path: &CanvasPsPath) -> Option<Bounds> {
    let points = polyline(path, CURVE_STEPS);
    let first = points.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for point in &points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some(Bounds::new(
        min_x,
        min_y,
        (max_x - min_x).max(1.0),
        (max_y - min_y).max(1.0),
    ))
}

/// `psPathSampler` (ps-path.ts:71): arc-length walk over the flattened path.
#[derive(Debug, Clone)]
pub struct PathSampler {
    points: Vec<Point>,
    lengths: Vec<f64>,
    pub total: f64,
}

impl PathSampler {
    pub fn new(path: &CanvasPsPath) -> Self {
        let points = polyline(path, CURVE_STEPS);
        let mut lengths = vec![0.0; points.len()];
        for index in 1..points.len() {
            let from = points[index - 1];
            let to = points[index];
            lengths[index] = lengths[index - 1] + (to.x - from.x).hypot(to.y - from.y);
        }
        let total = lengths.last().copied().unwrap_or(0.0);
        Self {
            points,
            lengths,
            total,
        }
    }

    fn locate(&self, length: f64) -> usize {
        if self.points.len() < 2 || self.total <= 0.0 {
            return 0;
        }
        let mut index = 1;
        while index < self.lengths.len().saturating_sub(1) && self.lengths[index] < length {
            index += 1;
        }
        index
    }

    /// Position at an arc length; `None` for an empty path.
    pub fn point_at(&self, length: f64) -> Option<Point> {
        let first = *self.points.first()?;
        if self.points.len() < 2 || self.total <= 0.0 {
            return Some(first);
        }
        let clamped = length.clamp(0.0, self.total);
        let index = self.locate(clamped).min(self.points.len() - 1);
        if index == 0 {
            return Some(self.points[0]);
        }
        let previous = (self.lengths[index] - self.lengths[index - 1]).max(0.0);
        let previous = if previous > 0.0 { previous } else { 1.0 };
        let ratio = (clamped - self.lengths[index - 1]) / previous;
        let from = self.points[index - 1];
        let to = self.points[index];
        Some(Point::new(
            from.x + (to.x - from.x) * ratio,
            from.y + (to.y - from.y) * ratio,
        ))
    }

    /// Tangent angle in degrees at an arc length.
    pub fn angle_at(&self, length: f64) -> f64 {
        if self.points.len() < 2 {
            return 0.0;
        }
        let index = self
            .locate(length.clamp(0.0, self.total))
            .min(self.points.len() - 1);
        let from = self.points[index.saturating_sub(1)];
        let to = self.points[index];
        (to.y - from.y).atan2(to.x - from.x).to_degrees()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::doc::Position;

    fn line_path() -> CanvasPsPath {
        create_path(
            "p",
            "Line",
            vec![path_anchor(0.0, 0.0), path_anchor(30.0, 40.0)],
            false,
        )
    }

    #[test]
    fn polyline_flattens_and_ends_on_last_anchor() {
        let path = line_path();
        let points = polyline(&path, 4);
        assert_eq!(points[0], Point::new(0.0, 0.0));
        assert_eq!(points[points.len() - 1], Point::new(30.0, 40.0));
    }

    #[test]
    fn bounds_cover_the_curve() {
        let path = create_path(
            "p",
            "C",
            vec![
                CanvasPsPathAnchor {
                    x: 0.0,
                    y: 0.0,
                    handle_in: Position { x: 0.0, y: 0.0 },
                    handle_out: Position { x: 10.0, y: 0.0 },
                },
                CanvasPsPathAnchor {
                    x: 20.0,
                    y: 0.0,
                    handle_in: Position { x: -10.0, y: 0.0 },
                    handle_out: Position { x: 0.0, y: 0.0 },
                },
            ],
            false,
        );
        let bounds = path_bounds(&path).expect("bounds");
        assert!(bounds.width >= 20.0);
        assert_eq!(bounds.y, 0.0);
    }

    #[test]
    fn sampler_walks_by_arc_length() {
        let path = line_path();
        let sampler = PathSampler::new(&path);
        assert!((sampler.total - 50.0).abs() < 1e-6);
        let mid = sampler.point_at(25.0).expect("mid");
        assert!((mid.x - 15.0).abs() < 1e-6);
        assert!((mid.y - 20.0).abs() < 1e-6);
        let angle = sampler.angle_at(25.0);
        assert!((angle - 53.130_102_355).abs() < 1e-3);
    }

    #[test]
    fn path_data_formats_polyline() {
        let path = line_path();
        let data = path_data(&path, Point::new(0.0, 0.0));
        assert!(data.starts_with("M 0.00 0.00"));
        assert!(data.contains("30.00 40.00"));
        assert!(!data.contains('Z'));
    }
}
