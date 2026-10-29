//! Screen-space geometry for the canvas overlays: ruler ticks. Pure functions
//! of the viewport and the canvas size, so the `.slint` side only renders the
//! rows these helpers produce.

use crate::coords::Viewport;

/// Ruler thickness (`RULER_THICKNESS`, canvas-rulers.tsx:6).
pub const RULER_THICKNESS: f64 = 20.0;
/// Minimum on-screen tick spacing (`MIN_TICK_SPACING`, canvas-rulers.tsx:7).
const MIN_TICK_SPACING: f64 = 56.0;
/// Base tick interval in world units (`BASE_TICK`, canvas-rulers.tsx:8).
const BASE_TICK: f64 = 100.0;

/// One ruler tick: on-screen position along the ruler axis plus its world value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    pub screen: f64,
    pub value: i64,
}

/// Ticks for both rulers (canvas-rulers.tsx:14-26). The step is `100 * k`-scaled
/// so labels never sit closer than [`MIN_TICK_SPACING`] screen pixels, and ticks
/// hidden under the other ruler (the corner where they overlap) are skipped.
pub fn ruler_ticks(viewport: Viewport, width: f64, height: f64) -> (Vec<Tick>, Vec<Tick>) {
    let step = BASE_TICK
        * (MIN_TICK_SPACING / (BASE_TICK * viewport.k))
            .ceil()
            .max(1.0);
    let x = axis_ticks(viewport.x, viewport.k, step, width, RULER_THICKNESS);
    let y = axis_ticks(viewport.y, viewport.k, step, height, 0.0);
    (x, y)
}

/// Ticks along one axis: `offset` is the viewport translation on that axis, the
/// screen position of a world value is `offset + value * k`, and values whose
/// screen position falls before `skip` (under the perpendicular ruler) are
/// dropped.
#[expect(
    clippy::cast_possible_truncation,
    reason = "tick values are whole world-unit multiples; the f64 round is exact for them"
)]
fn axis_ticks(offset: f64, k: f64, step: f64, extent: f64, skip: f64) -> Vec<Tick> {
    let mut ticks = Vec::new();
    // The step advances the screen position by at least MIN_TICK_SPACING per
    // iteration, so the loop is bounded by extent / MIN_TICK_SPACING.
    let mut value = (-offset / k / step).floor() * step;
    loop {
        let screen = offset + value * k;
        if screen > extent {
            break;
        }
        if screen >= skip {
            ticks.push(Tick {
                screen,
                value: value.round() as i64,
            });
        }
        value += step;
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_ticks_start_at_the_first_visible_hundred() {
        let (x, y) = ruler_ticks(Viewport::new(), 1040.0, 744.0);
        // The 0 tick is under the left ruler (screen < 20) and is skipped.
        assert_eq!(
            x.first(),
            Some(&Tick {
                screen: 100.0,
                value: 100
            })
        );
        assert_eq!(
            x.last(),
            Some(&Tick {
                screen: 1000.0,
                value: 1000
            })
        );
        assert_eq!(
            y.first(),
            Some(&Tick {
                screen: 0.0,
                value: 0
            })
        );
        assert_eq!(
            y.last(),
            Some(&Tick {
                screen: 700.0,
                value: 700
            })
        );
    }

    #[test]
    fn ruler_step_grows_with_zoom_out_and_tracks_pan() {
        let zoomed_out = Viewport {
            x: 0.0,
            y: 0.0,
            k: 0.5,
        };
        let (x, _) = ruler_ticks(zoomed_out, 1040.0, 744.0);
        // 56 / (100 * 0.5) = 1.12 -> step 200, drawn at value * 0.5.
        assert_eq!(
            x.first(),
            Some(&Tick {
                screen: 100.0,
                value: 200
            })
        );
        assert_eq!(
            x.last(),
            Some(&Tick {
                screen: 1000.0,
                value: 2000
            })
        );

        // Panning by -150 shifts every tick by -150 screen pixels.
        let panned = Viewport {
            x: -150.0,
            y: 0.0,
            k: 1.0,
        };
        let (x, _) = ruler_ticks(panned, 1040.0, 744.0);
        assert_eq!(
            x.first(),
            Some(&Tick {
                screen: 50.0,
                value: 200
            })
        );
    }
}
