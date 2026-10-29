//! Marching-ants phase tests (`ps-overlays.tsx:21,52,60`): the dash phase
//! advances every 120ms and the light half of the pattern follows it.

use crate::image::{
    ants_advance, ants_light, ANTS_DASH, ANTS_PHASE_CYCLE, ANTS_PHASE_STEP, ANTS_TICK_MS,
};

#[test]
fn the_phase_steps_one_pixel_per_tick_and_wraps() {
    assert_eq!(ANTS_TICK_MS, 120, "the web's ANT_TICK_MS");
    assert_eq!(ANTS_PHASE_CYCLE, ANTS_DASH * 2);
    assert_eq!(ants_advance(0), ANTS_PHASE_STEP);
    assert_eq!(ants_advance(ANTS_PHASE_CYCLE - ANTS_PHASE_STEP), 0);
    // A full cycle of ticks returns to the start.
    let mut phase = 0;
    for _ in 0..ANTS_PHASE_CYCLE {
        phase = ants_advance(phase);
    }
    assert_eq!(phase, 0);
}

#[test]
fn the_light_dashes_march_with_the_phase() {
    // Phase 0 is the desktop's original static pattern: the light half covers
    // the second 2px run of every 4.
    assert!(!ants_light(0, 0, 0));
    assert!(!ants_light(1, 0, 0));
    assert!(ants_light(2, 0, 0));
    assert!(ants_light(3, 0, 0));
    assert!(!ants_light(4, 0, 0));
    // One tick shifts the pattern one pixel along the boundary.
    assert!(ants_light(1, 0, ANTS_PHASE_STEP));
    assert!(!ants_light(3, 0, ANTS_PHASE_STEP));
    assert_ne!(ants_light(0, 0, 0), ants_light(2, 0, ANTS_PHASE_STEP));
    // The pattern runs diagonally, like the web's `(x + y + phase)`.
    assert_eq!(ants_light(3, 5, 0), ants_light(8, 0, 0));
}
