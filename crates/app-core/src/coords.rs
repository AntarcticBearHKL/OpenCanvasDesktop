//! Screen and world coordinate spaces plus the viewport transform.
//!
//! `WorldPos` is canvas/document space at scale 1; `ScreenPos` is viewport space
//! (logical pixels inside the canvas area). The two are distinct newtypes so the
//! compiler refuses to mix them, and every conversion goes through [`Viewport`].

/// A point in canvas (world) space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPos {
    pub x: f64,
    pub y: f64,
}

impl WorldPos {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A point in viewport (screen) space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPos {
    pub x: f64,
    pub y: f64,
}

impl ScreenPos {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The canvas viewport: translation plus zoom factor `k` (web `ViewportTransform`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub k: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewport {
    pub const MIN_K: f64 = 0.05;
    pub const MAX_K: f64 = 5.0;

    pub const fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        }
    }

    /// `screenToCanvas` (project.tsx:446-455).
    pub fn screen_to_world(&self, p: ScreenPos) -> WorldPos {
        WorldPos::new((p.x - self.x) / self.k, (p.y - self.y) / self.k)
    }

    pub fn world_to_screen(&self, p: WorldPos) -> ScreenPos {
        ScreenPos::new(p.x * self.k + self.x, p.y * self.k + self.y)
    }

    /// Wheel zoom anchored at the cursor (open-canvas.tsx:126-149):
    /// `factor = 1.1^(delta/100)`, `k` clamped to `0.05..=5`, and the world point
    /// under the cursor stays under the cursor.
    pub fn zoom_at(&mut self, anchor: ScreenPos, delta: f64) {
        let factor = 1.1_f64.powf(delta / 100.0);
        let next_k = (self.k * factor).clamp(Self::MIN_K, Self::MAX_K);
        let world = self.screen_to_world(anchor);
        self.k = next_k;
        self.x = anchor.x - world.x * next_k;
        self.y = anchor.y - world.y * next_k;
    }

    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_screen_round_trip_through_the_viewport() {
        let viewport = Viewport {
            x: 120.0,
            y: -40.0,
            k: 2.0,
        };
        let world = WorldPos::new(30.0, 55.0);
        let screen = viewport.world_to_screen(world);
        assert_eq!(screen, ScreenPos::new(180.0, 70.0));
        assert_eq!(viewport.screen_to_world(screen), world);
    }

    #[test]
    fn zoom_keeps_the_world_point_under_the_cursor() {
        let mut viewport = Viewport::new();
        let cursor = ScreenPos::new(400.0, 300.0);
        let before = viewport.screen_to_world(cursor);
        viewport.zoom_at(cursor, 100.0);
        assert!((viewport.k - 1.1).abs() < 1e-9);
        assert_eq!(viewport.screen_to_world(cursor), before);
    }

    #[test]
    fn zoom_scale_is_clamped_to_the_web_range() {
        let mut viewport = Viewport::new();
        viewport.zoom_at(ScreenPos::new(0.0, 0.0), 10_000.0);
        assert_eq!(viewport.k, Viewport::MAX_K);
        viewport.zoom_at(ScreenPos::new(0.0, 0.0), -100_000.0);
        assert_eq!(viewport.k, Viewport::MIN_K);
    }
}
