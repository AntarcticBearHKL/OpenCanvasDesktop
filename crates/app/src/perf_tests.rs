//! Timing guards for the hot UI paths (canvas sync, node rows, studio render).
//!
//! These are not micro-benchmarks: each one times the function the app really
//! calls, on a fixture sized like a busy project (200 nodes / 400 wires, a
//! 1024x768 board with four layers). The budgets are deliberately loose — they
//! guard against a regression of an order of magnitude on the `dev` profile
//! (workspace crates are `opt-level = 0`, so absolute numbers are several times
//! the release ones), not against a few percent of noise.
//!
//! Run only these with `cargo test -p open-canvas-app perf -- --nocapture`.

use std::time::{Duration, Instant};

use slint::ComponentHandle;

use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata, CanvasNodeType, Position,
};

use crate::app::App;
use crate::coords::ScreenPos;
use crate::input::{Button, PointerEvent, PointerKind};
use crate::store::Store;
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Median wall time of `runs` calls, so a single hiccup cannot fail a budget.
fn median(runs: usize, mut body: impl FnMut()) -> Duration {
    let mut samples: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            body();
            start.elapsed()
        })
        .collect();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// A test-suite timing line: the optimiser reads these in the run output.
fn report(label: &str, time: Duration) {
    println!("perf {label}: {:.2} ms", time.as_secs_f64() * 1_000.0);
}

/// A pixel coordinate as `Surface::set` takes it (no `as` casts).
fn coord(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Grid position of node `index` on the fixture board.
fn grid_position(index: usize) -> Position {
    let column = crate::vm::i2f(crate::vm::u2i(index % 20));
    let row = crate::vm::i2f(crate::vm::u2i(index / 20));
    Position::new(column * 360.0, row * 280.0)
}

fn text_node(index: usize) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::Text,
        title: format!("Text {index}"),
        position: grid_position(index),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            texts: Some(vec![
                open_canvas_core::node::CanvasNodeText {
                    id: format!("t{index}"),
                    content: format!("Body text of node {index}"),
                    ..open_canvas_core::node::CanvasNodeText::default()
                },
                open_canvas_core::node::CanvasNodeText {
                    id: format!("t{index}b"),
                    content: format!("Second option of node {index}"),
                    ..open_canvas_core::node::CanvasNodeText::default()
                },
            ]),
            font_size: Some(14.0),
            ..CanvasNodeMetadata::default()
        }),
    }
}

/// An image node with a tiny shared data URL (the media cache hits after the
/// first decode, exactly like a project whose bitmaps are already on screen).
fn image_node(index: usize, content: &str) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::Image,
        title: format!("Image {index}"),
        position: grid_position(index),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            content: Some(content.to_string()),
            thumbnail: Some(content.to_string()),
            natural_width: Some(512.0),
            natural_height: Some(384.0),
            bytes: Some(48_000),
            ..CanvasNodeMetadata::default()
        }),
    }
}

fn stack_node(index: usize, content: &str) -> CanvasNodeData {
    let images = (0..5)
        .map(|slot| CanvasNodeImage {
            id: format!("n{index}-{slot}"),
            content: content.to_string(),
            thumbnail: Some(content.to_string()),
            status: open_canvas_core::node::CanvasNodeStatus::Success,
            natural_width: 512.0,
            natural_height: 384.0,
            ..CanvasNodeImage::default()
        })
        .collect();
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::ImageStack,
        title: format!("Stack {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            images: Some(images),
            primary_image_id: Some(format!("n{index}-0")),
            ..CanvasNodeMetadata::default()
        }),
    }
}

fn prompt_node(index: usize) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::Prompt,
        title: format!("Prompt {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            prompt: Some(format!("a wide shot of scene {index}, soft light")),
            references: Some(vec![
                format!("n{}", index.wrapping_sub(1)),
                format!("n{}", index.wrapping_sub(2)),
            ]),
            ..CanvasNodeMetadata::default()
        }),
    }
}

fn generation_node(index: usize, kind: CanvasNodeType) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: kind,
        title: format!("Generation {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            quality: Some("high".to_string()),
            size: Some("1024x768".to_string()),
            count: Some(1),
            ..CanvasNodeMetadata::default()
        }),
    }
}

fn video_prompt_node(index: usize) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::VideoPrompt,
        title: format!("Video prompt {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            prompt: Some("a slow pan over the harbour at dusk".to_string()),
            video_mode: Some(open_canvas_core::node::CanvasVideoMode::Frames),
            video_slots: Some(open_canvas_core::node::CanvasVideoSlots {
                first_frame: Some(format!("n{}", index.wrapping_sub(1))),
                last_frame: Some(format!("n{}", index.wrapping_sub(2))),
                references: None,
            }),
            ..CanvasNodeMetadata::default()
        }),
    }
}

fn assets_node(index: usize) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::Assets,
        title: format!("Assets {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata::default()),
    }
}

fn sticky_node(index: usize) -> CanvasNodeData {
    CanvasNodeData {
        id: format!("n{index}"),
        node_type: CanvasNodeType::Plugin("official:sticky-note".to_string()),
        title: format!("Note {index}"),
        position: Position::new(0.0, 0.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            content: Some(format!("remember to check take {index}")),
            ..CanvasNodeMetadata::default()
        }),
    }
}

/// 200 nodes of a mixed project: text/prompt/image/stack/generation/assets and
/// a few plugin cards, wired as `image -> prompt -> generation` triples plus
/// a reference chain, for 400 connections in total.
fn fixture(count: usize, image_content: &str) -> (Vec<CanvasNodeData>, Vec<CanvasConnection>) {
    let mut nodes = Vec::with_capacity(count);
    for index in 0..count {
        let node = match index % 10 {
            0 => prompt_node(index),
            1 | 4 => image_node(index, image_content),
            2 | 7 => text_node(index),
            3 => generation_node(index, CanvasNodeType::ImageGeneration),
            5 => stack_node(index, image_content),
            6 => video_prompt_node(index),
            8 => assets_node(index),
            _ => sticky_node(index),
        };
        nodes.push(node);
    }
    let mut connections = Vec::with_capacity(count * 2);
    for index in 0..count * 2 {
        let from = index % count;
        let to = (index * 7 + 3) % count;
        if from == to {
            continue;
        }
        connections.push(CanvasConnection {
            id: format!("c{index}"),
            from_node_id: format!("n{from}"),
            to_node_id: format!("n{to}"),
            relation: Some(if index % 3 == 0 {
                "reference".to_string()
            } else {
                "prompt".to_string()
            }),
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        });
    }
    (nodes, connections)
}

/// A 4x4 red PNG as a data URL, the fixture media of every image card.
fn tiny_png() -> Result<String, Box<dyn std::error::Error>> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    let mut surface = open_canvas_ps::Surface::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            surface.set(x, y, [220, 60, 40, 255]);
        }
    }
    let png = open_canvas_ps::encode_png(&surface)?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

fn fixture_app(count: usize) -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
    let content = tiny_png()?;
    let (nodes, connections) = fixture(count, &content);
    let dir = tempfile::TempDir::new()?;
    let mut app = App::new(Store::at(dir.path().to_path_buf()));
    app.doc = crate::doc::Doc::assemble(nodes, connections);
    let rows = app.visible_rows();
    app.rows.set_vec(rows);
    app.doc.select_node("n3", false);
    Ok((app, dir))
}

#[test]
fn rows_full_rebuild_200_nodes() -> TestResult {
    let (mut app, _dir) = fixture_app(200)?;
    let time = median(3, || {
        std::hint::black_box(app.rows_for_nodes(None));
    });
    report("rows full rebuild (200 nodes / 400 wires)", time);
    assert!(
        time < Duration::from_millis(400),
        "a full row rebuild of 200 nodes stays under 400 ms in the dev profile ({time:?})"
    );
    Ok(())
}

#[test]
fn rows_single_node_edit_200_nodes() -> TestResult {
    let (mut app, _dir) = fixture_app(200)?;
    let ids = [app.doc.nodes()[40].id.clone()];
    let time = median(5, || {
        std::hint::black_box(app.rows_for_nodes(Some(&ids)));
    });
    report("rows single-node edit (200 nodes)", time);
    assert!(
        time < Duration::from_millis(40),
        "one edited node never costs a full rebuild plus a 200-row scan ({time:?})"
    );
    Ok(())
}

#[test]
fn wire_commands_60_pans() -> TestResult {
    let (app, _dir) = fixture_app(200)?;
    let nodes = app.doc.nodes().to_vec();
    let connections = app.doc.connections().to_vec();
    let time = median(3, || {
        for _ in 0..60 {
            std::hint::black_box(crate::paths::wires_commands(&nodes, &connections));
        }
    });
    report("60 wire command rebuilds (400 wires)", time);
    assert!(
        time < Duration::from_millis(1_200),
        "60 wire path rebuilds stay under 1.2 s ({time:?})"
    );
    Ok(())
}

#[test]
fn sync_view_60_pans() -> TestResult {
    let dir = tempfile::TempDir::new()?;
    crate::test_support::init_backend_allow_existing();
    let ui = crate::AppWindow::new()?;
    let content = tiny_png()?;
    let (nodes, connections) = fixture(200, &content);
    let mut app = App::new(Store::at(dir.path().to_path_buf()));
    app.doc = crate::doc::Doc::assemble(nodes, connections);
    let rows = app.visible_rows();
    app.rows.set_vec(rows);
    let time = median(3, || {
        for step in 0..60 {
            app.doc
                .pan_to(crate::coords::ScreenPos::new(f64::from(step), 0.0));
            app.sync_view(&ui);
        }
    });
    report("60 full sync_view pans (200 nodes / 400 wires)", time);
    assert!(
        time < Duration::from_secs(3),
        "60 pan frames stay under 3 s ({time:?})"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Canvas background: pre-rendered dot tile vs the world-space grid path
// ---------------------------------------------------------------------------

type Buffer = slint::SharedPixelBuffer<slint::Rgba8Pixel>;
/// Pan frames per case in the grid render benchmark.
const GRID_BENCH_FRAMES: usize = 10;

/// One editor window under test: its temp store, the window and the shared app.
type EditorWindow = (
    tempfile::TempDir,
    crate::AppWindow,
    std::rc::Rc<std::cell::RefCell<App>>,
);

/// One measured grid case: zoom label, implementation, per-frame time, lattice
/// image count and the bitmap key (only the tiled case has one).
type GridCase<'a> = (
    &'a str,
    &'a str,
    Duration,
    i32,
    Option<crate::grid_tile::GridTileKey>,
);

/// Ink of one rendered canvas region: the lattice phase (dot position modulo
/// the spacing) and the total ink mass. Comparing these proves the tiled
/// background draws the *same* lattice as the old path, without needing a
/// byte-identical rasterisation.
#[derive(Debug, Clone, Copy)]
struct Ink {
    phase_x: f64,
    phase_y: f64,
    mass: f64,
    pixels: usize,
    darkest: f64,
}

/// Measures [`Ink`] in `region` (x0, y0, x1, y1). A pixel counts as ink when it
/// is at least 12 levels darker than the canvas field, i.e. a dot; the canvas is
/// empty in this benchmark, so nothing else paints there.
fn ink(buffer: &Buffer, region: (u32, u32, u32, u32), spacing: f64) -> Ink {
    let (x0, y0, x1, y1) = region;
    let field = 248.0;
    let (mut mass, mut pixels) = (0.0f64, 0usize);
    let mut darkest = 255.0f64;
    let (mut cos_x, mut sin_x, mut cos_y, mut sin_y) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let tau = std::f64::consts::TAU;
    for y in y0..y1.min(buffer.height()) {
        for x in x0..x1.min(buffer.width()) {
            let pixel = buffer.as_slice()[(y * buffer.width() + x) as usize];
            let luma = 0.299 * f64::from(pixel.r)
                + 0.587 * f64::from(pixel.g)
                + 0.114 * f64::from(pixel.b);
            let weight = field - luma;
            if weight <= 12.0 {
                continue;
            }
            pixels += 1;
            mass += weight;
            darkest = darkest.min(luma);
            let angle_x = tau * f64::from(x) / spacing;
            let angle_y = tau * f64::from(y) / spacing;
            cos_x += weight * angle_x.cos();
            sin_x += weight * angle_x.sin();
            cos_y += weight * angle_y.cos();
            sin_y += weight * angle_y.sin();
        }
    }
    let wrap = |v: f64| {
        let mut v = v;
        while v < 0.0 {
            v += spacing;
        }
        while v >= spacing {
            v -= spacing;
        }
        v
    };
    if pixels == 0 {
        return Ink {
            phase_x: f64::NAN,
            phase_y: f64::NAN,
            mass: 0.0,
            pixels: 0,
            darkest: 255.0,
        };
    }
    Ink {
        phase_x: wrap(sin_x.atan2(cos_x) / tau * spacing),
        phase_y: wrap(sin_y.atan2(cos_y) / tau * spacing),
        mass,
        pixels,
        darkest,
    }
}

/// Circular distance between two lattice phases, in pixels.
fn phase_gap(a: f64, b: f64, spacing: f64) -> f64 {
    let diff = (a - b) % spacing;
    diff.abs().min(spacing - diff.abs())
}

/// A shown editor window with an empty document: the canvas is nothing but the
/// dot grid, which is what this benchmark wants to isolate.
fn empty_editor() -> Result<EditorWindow, Box<dyn std::error::Error>> {
    crate::test_support::init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = crate::AppWindow::new()?;
    let app = std::rc::Rc::new(std::cell::RefCell::new(App::new(Store::at(
        dir.path().to_path_buf(),
    ))));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    ui.set_view("editor".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    // Let the layout run once so `canvas_size` matches what is painted.
    drop(ui.window().take_snapshot()?);
    Ok((dir, ui, app))
}

/// A region safely inside the canvas: right of the 280 px side panel, below the
/// top bar, and clear of the bottom dock.
const CANVAS_REGION: (u32, u32, u32, u32) = (420, 140, 1010, 470);

/// The tiled background is screen-anchored: a zoom sweep changes neither the
/// spacing, nor the dot weight, nor the lattice phase, and a pan only slides the
/// lattice by the pan distance, rounded to whole pixels (no shimmer).
#[test]
fn the_tiled_grid_is_screen_anchored_and_pans_without_shimmer() -> TestResult {
    let (_dir, ui, app) = empty_editor()?;
    let spacing = crate::paths::GRID_SCREEN_SPACING;
    let mut measured: Vec<(&str, Ink)> = Vec::new();
    for (label, k, pan) in [
        ("100 %", 1.0, 0.0),
        ("400 %", 4.0, 0.0),
        ("500 %", 5.0, 0.0),
        ("25 %", 0.25, 0.0),
        ("100 % + pan", 1.0, 17.37),
        ("500 % + pan", 5.0, 17.37),
    ] {
        crate::app::with_app(&app, |app| {
            app.legacy_grid = false;
            app.doc.viewport = crate::coords::Viewport {
                x: -pan,
                y: -pan / 2.0,
                k,
            };
            app.sync_view(&ui);
        });
        ui.window().request_redraw();
        let stats = ink(&ui.window().take_snapshot()?, CANVAS_REGION, spacing);
        eprintln!("grid ink @{label}: {stats:?}");
        measured.push((label, stats));
    }
    for (label, stats) in &measured {
        assert!(
            stats.pixels > 100,
            "{label}: the region shows the dot grid ({stats:?})"
        );
        // Same dot in every frame: the constant 2.5 px dot at the theme's 0.40
        // ink * 0.65 layer opacity.
        assert!(
            stats.darkest < 246.0,
            "{label}: the dots are painted ({stats:?})"
        );
        // One dot per `spacing^2` of canvas: a wrong tile size or spacing would
        // change how much ink the region holds.
        let (x0, y0, x1, y1) = CANVAS_REGION;
        let region = f64::from(x1 - x0) * f64::from(y1 - y0);
        #[expect(
            clippy::cast_precision_loss,
            reason = "pixel counts in a small snapshot region are exact in f64"
        )]
        let per_dot = stats.pixels as f64 / (region / (spacing * spacing));
        assert!(
            (2.0..=25.0).contains(&per_dot),
            "{label}: {per_dot:.1} inked pixels per dot is a 2.5 px dot ({stats:?})"
        );
    }
    // Zooming never touches the lattice: the same phase, the same ink, the same
    // dot shade at 25 %, 100 %, 400 % and 500 %.
    let base = measured[0].1;
    for (label, stats) in measured.iter().filter(|(label, _)| !label.contains("pan")) {
        assert!(
            phase_gap(stats.phase_x, base.phase_x, spacing) < 0.5
                && phase_gap(stats.phase_y, base.phase_y, spacing) < 0.5,
            "{label}: the lattice phase is a screen constant ({stats:?} vs {base:?})"
        );
        assert!(
            (stats.mass / base.mass - 1.0).abs() < 0.02,
            "{label}: the same dots at every zoom ({:.1} vs {:.1})",
            stats.mass,
            base.mass
        );
        assert!(
            (stats.darkest - base.darkest).abs() < 1.0,
            "{label}: the same dot shade at every zoom ({:.2} vs {:.2})",
            stats.darkest,
            base.darkest
        );
    }
    // Panning slides the lattice by the pan, snapped to a whole pixel: 17.37 px
    // of pan moves the dots 17 px, 8.685 px moves them 9 px. The viewport
    // offset is `-pan`, so both shifts are negative here; wrap into one tile to
    // read the magnitude (a 17 px shift past the tile edge stays 17, not 9).
    let shifted = measured
        .iter()
        .find(|(label, _)| *label == "100 % + pan")
        .map(|(_, stats)| *stats)
        .expect("the panned frame");
    let shift = |a: f64, b: f64| ((a - b + spacing).rem_euclid(2.0 * spacing)) - spacing;
    let (shift_x, shift_y) = (
        shift(shifted.phase_x, base.phase_x),
        shift(shifted.phase_y, base.phase_y),
    );
    assert!(
        (shift_x.abs() - 17.0).abs() < 1.0,
        "the lattice follows the x pan, snapped to whole pixels ({shift_x:.2})"
    );
    assert!(
        (shift_y.abs() - 9.0).abs() < 1.0,
        "the lattice follows the y pan, snapped to whole pixels ({shift_y:.2})"
    );
    assert!(
        (shifted.darkest - base.darkest).abs() < 1.0,
        "panning does not change a dot's shade (no shimmer): {:.2} vs {:.2}",
        shifted.darkest,
        base.darkest
    );
    Ok(())
}

/// Old vs new grid cost, measured on the real window with the software
/// renderer: ten 3.7 px pan frames per case, one `take_snapshot` each. The
/// numbers are *reported*; the assertions only guard a pathological regression
/// (the software snapshot is ~30 ms of unrelated chrome and moves by more than
/// the grid's share under load) plus the renderer-independent facts: the tile
/// bitmap is generated once, and the old path had thousands of segments to
/// re-tessellate per frame while the tiled background has none. The
/// tiled background reuses one bitmap and only moves a dozen `Image` elements;
/// the path re-tessellates every dot segment per frame, and the segment count
/// grows as the canvas zooms out (the stride keeps the on-screen spacing at
/// least 12 px wide, so 100 % is already a few thousand segments and 25 % is
/// thousands more).
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one benchmark: two zooms x two implementations, plus the isolated grid-work figures"
)]
fn the_tiled_grid_render_costs_no_more_than_the_path() -> TestResult {
    let (_dir, ui, app) = empty_editor()?;
    let mut results: Vec<GridCase> = Vec::new();
    for (zoom_label, k) in [("100 %", 1.0), ("25 %", 0.25)] {
        for legacy in [true, false] {
            // Warm-up frame: font/image caches are shared with the rest of the
            // window and must not land on the first measured frame.
            crate::app::with_app(&app, |app| {
                app.legacy_grid = legacy;
                app.doc.viewport = crate::coords::Viewport { x: 0.0, y: 0.0, k };
                app.sync_view(&ui);
            });
            ui.window().request_redraw();
            drop(ui.window().take_snapshot()?);

            let mut frames: Vec<Duration> = Vec::with_capacity(GRID_BENCH_FRAMES);
            let mut first_key: Option<crate::grid_tile::GridTileKey> = None;
            let mut last_key: Option<crate::grid_tile::GridTileKey> = None;
            for step in 0..GRID_BENCH_FRAMES {
                let pan = f64::from(u32::try_from(step).unwrap_or(0)) * 3.7;
                crate::app::with_app(&app, |app| {
                    app.legacy_grid = legacy;
                    app.doc.viewport = crate::coords::Viewport {
                        x: -pan,
                        y: pan / 2.0,
                        k,
                    };
                    app.sync_view(&ui);
                });
                let key =
                    crate::app::with_app_r(&app, |app| app.grid_tile.as_ref().map(|tile| tile.key))
                        .flatten();
                if first_key.is_none() {
                    first_key = key;
                }
                last_key = key;
                ui.window().request_redraw();
                let started = Instant::now();
                let _snapshot = ui.window().take_snapshot()?;
                frames.push(started.elapsed());
            }
            // Median, not total: the full-window snapshot is ~30 ms of chrome
            // and a single scheduling hiccup must not decide the comparison.
            frames.sort_unstable();
            let per_frame = frames.get(frames.len() / 2).copied().unwrap_or_default();
            eprintln!(
                "grid render @{zoom_label}: {} = {:.2} ms/frame (median of {GRID_BENCH_FRAMES}), lattice {}x{}",
                if legacy { "world-space Path" } else { "tiled Image   " },
                per_frame.as_secs_f64() * 1000.0,
                ui.get_grid_cols(),
                ui.get_grid_rows()
            );
            if !legacy {
                assert_eq!(
                    first_key, last_key,
                    "the tile bitmap is generated once for all {GRID_BENCH_FRAMES} pan frames"
                );
            }
            results.push((
                zoom_label,
                if legacy { "path" } else { "tile" },
                per_frame,
                ui.get_grid_cols() * ui.get_grid_rows(),
                last_key,
            ));
        }
    }
    // The work itself, without the window around it: how many dot segments the
    // old path had to tessellate, how long building that command string took,
    // and how long the one tile bitmap takes to generate (once per run).
    for (label, k) in [("100 %", 1.0), ("25 %", 0.25)] {
        let viewport = crate::coords::Viewport { x: 0.0, y: 0.0, k };
        let patch = crate::paths::grid_patch(viewport, 1000.0, 744.0);
        let segments = patch.commands.matches("M ").count();
        let build = median(20, || {
            std::hint::black_box(crate::paths::grid_patch(viewport, 1000.0, 744.0));
        });
        let layout = crate::paths::grid_tile_layout(1000.0, 744.0);
        let key = crate::grid_tile::GridTileKey::of(
            &layout,
            1.0,
            slint::Color::from_argb_u8(102, 58, 48, 40),
        );
        let raster = median(20, || {
            std::hint::black_box(crate::grid_tile::rasterize(&layout, key));
        });
        eprintln!(
            "grid work @{label}: path {segments} dot segments, command string {:.3} ms/frame; tile {}x{} dots, {}px bitmap, generated {:.3} ms once",
            build.as_secs_f64() * 1000.0,
            layout.dots,
            layout.dots,
            key.image_px,
            raster.as_secs_f64() * 1000.0
        );
        assert!(
            segments > 500,
            "@{label}: the old path really had a segment per dot"
        );
    }
    for pair in results.chunks(2) {
        let (zoom, path_name, path_frame, _, _) = pair[0];
        let (_, tile_name, tile_frame, images, key) = pair[1];
        eprintln!(
            "grid render @{zoom}: {tile_name} is {:.2}x cheaper than {path_name} ({images} images of a {}px tile)",
            path_frame.as_secs_f64() / tile_frame.as_secs_f64().max(1e-9),
            key.map_or(0, |key| key.image_px)
        );
        assert!(path_name == "path" && tile_name == "tile");
        assert!(images > 0, "the tiled background paints a lattice");
        assert!(key.is_some(), "the tiled background has a bitmap");
        // A pathological regression guard only: the real win is the per-pan
        // work (zero re-tessellation / zero command string) and grows with the
        // segment count. The full-window software snapshot itself is dominated
        // by ~30 ms of chrome and swings by tens of percent under load.
        assert!(
            tile_frame.as_secs_f64() <= path_frame.as_secs_f64() * 2.0,
            "@{zoom}: the tiled background stays in the same cost class as the path ({tile_frame:?} vs {path_frame:?})"
        );
    }
    Ok(())
}

/// The five cards a coalesced drag test moves, and where the pointer starts.
const DRAG_IDS: [&str; 5] = ["n3", "n13", "n23", "n33", "n43"];
const DRAG_START: (f64, f64) = (40_000.0, 40_000.0);

/// A node-drag frame budget at the 125 Hz tick: 8 ms frame, at most 3 ms of the
/// drag work (rows + dirty wires + overlays + one redraw), leaving >5 ms for the
/// rest of the app and the renderer at 120 fps. The budget is deliberately loose
/// (median of three passes) so a loaded test host does not decide the gate.
const DRAG_FRAME_BUDGET_MS: f64 = 3.0;

fn drag_pointer(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        kind,
        pos: ScreenPos::new(x, y),
        button: Button::Left,
        shift: false,
        ctrl: false,
    }
}

fn select_drag_nodes(app: &mut App) {
    for id in DRAG_IDS {
        app.doc.select_node(id, true);
    }
}

/// Connections touching one of the dragged cards (the wire paths a live tick
/// may rebuild).
fn dirty_wires(app: &App) -> usize {
    app.doc
        .connections()
        .iter()
        .filter(|connection| {
            DRAG_IDS.contains(&connection.from_node_id.as_str())
                || DRAG_IDS.contains(&connection.to_node_id.as_str())
        })
        .count()
}

/// The old per-event path, for the before number: every pointer sample applied
/// the drag and rebuilt the five rows, the whole wire model and the overlays.
fn per_event_drag(app: &mut App, ui: &AppWindow, moves: usize) -> Duration {
    let ids: Vec<String> = DRAG_IDS.iter().map(|id| (*id).to_string()).collect();
    app.doc
        .begin_node_drag(DRAG_IDS[0], ScreenPos::new(DRAG_START.0, DRAG_START.1));
    let mut x = DRAG_START.0;
    let mut y = DRAG_START.1;
    let started = Instant::now();
    for _ in 0..moves {
        x += 1.0;
        y += 0.5;
        app.doc.node_drag_to(ScreenPos::new(x, y));
        app.sync_rows_of(ui, &ids);
        app.sync_wires(ui);
        app.sync_overlays(ui);
        ui.window().request_redraw();
    }
    let elapsed = started.elapsed();
    app.doc.end_node_drag();
    elapsed
}

/// Starts a drag at `start` and runs 30 frames of 240 pointer moves in total.
/// Returns the frame time and the row writes of each frame; the drag is left
/// active so the caller can release it (or assert on its live state).
fn coalesced_drag_frames(
    app: &mut App,
    ui: &AppWindow,
    clock: &mut u64,
    start: (f64, f64),
) -> (Duration, Vec<u64>) {
    app.clock_override = Some(*clock);
    app.node_pointer(
        ui,
        DRAG_IDS[0],
        &drag_pointer(PointerKind::Down, start.0, start.1),
    );
    let mut pos = start;
    let mut writes = Vec::with_capacity(30);
    let started = Instant::now();
    for _ in 0..30 {
        let before = app.drag_stats.row_updates;
        for _ in 0..8 {
            pos.0 += 1.0;
            pos.1 += 0.5;
            *clock += 1;
            app.clock_override = Some(*clock);
            app.node_pointer(
                ui,
                DRAG_IDS[0],
                &drag_pointer(PointerKind::Move, pos.0, pos.1),
            );
        }
        *clock += 1;
        app.clock_override = Some(*clock);
        app.node_drag_tick(ui);
        writes.push(app.drag_stats.row_updates - before);
    }
    let elapsed = started.elapsed();
    (elapsed, writes)
}

/// Releases the drag started by [`coalesced_drag_frames`] at the exact end of
/// the move path (plus a sub-pixel offset that must not be quantised away).
fn release_drag(app: &mut App, ui: &AppWindow, clock: &mut u64, start: (f64, f64)) {
    *clock += 1;
    app.clock_override = Some(*clock);
    app.node_pointer(
        ui,
        DRAG_IDS[0],
        &drag_pointer(PointerKind::Up, start.0 + 240.25, start.1 + 120.125),
    );
}

/// The node-drag contract at full input rate: 240 pointer moves arrive in 30
/// frames (8 per frame), the tick applies each batch once, and the release lands
/// the exact final position. Measured against the old per-event path on the same
/// 200-node board with five selected cards.
///
/// Asserts: no row-model rebuild while dragging (the release is the one), no
/// wire text rebuild either, at most one redraw per frame, only dirty wire paths
/// regenerated, and the whole frame under 2 ms in the dev profile.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one benchmark: the documented before path, the measured coalesced path and the exact-release assertion"
)]
fn a_coalesced_node_drag_frame_stays_in_budget() -> TestResult {
    crate::test_support::init_backend_allow_existing();
    let ui = crate::AppWindow::new()?;

    // Before: one full row/wire/overlay sync per pointer sample.
    let (mut old_app, _old_dir) = fixture_app(200)?;
    old_app.sync_all(&ui);
    old_app.drag_stats.reset();
    let baseline = per_event_drag(&mut old_app, &ui, 240);
    let baseline_stats = old_app.drag_stats;
    let per_event_ms = baseline.as_secs_f64() * 1_000.0 / 240.0;
    eprintln!(
        "node drag before (per event): {:.2} ms / 240 moves = {:.3} ms/event; at 1000 Hz that is {:.0} ms/s, {} row refreshes and {} wire-string rebuilds",
        baseline.as_secs_f64() * 1_000.0,
        per_event_ms,
        per_event_ms * 1_000.0,
        baseline_stats.row_rebuilds,
        baseline_stats.wire_pushes,
    );

    // After: 240 moves coalesced into 30 ticks, then the release. Two warm-up
    // passes (each with a real release) precede the counted one; the median of
    // the three per-frame times keeps a loaded test host from failing the
    // budget, exactly like the other perf guards.
    let (mut app, _dir) = fixture_app(200)?;
    app.sync_all(&ui);
    select_drag_nodes(&mut app);
    app.sync_rows_full(&ui);
    let dirty = dirty_wires(&app);
    assert!(dirty > 0, "the dragged cards have wires to rebuild");

    let mut clock = 10_000u64;
    let mut timings: Vec<Duration> = Vec::with_capacity(3);
    for _ in 0..2 {
        let (elapsed, _) = coalesced_drag_frames(&mut app, &ui, &mut clock, DRAG_START);
        timings.push(elapsed);
        release_drag(&mut app, &ui, &mut clock, DRAG_START);
    }
    app.drag_stats.reset();
    let initial_nodes = app.doc.nodes().to_vec();
    let initial_connections = app.doc.connections().to_vec();
    let (frames, frame_writes) = coalesced_drag_frames(&mut app, &ui, &mut clock, DRAG_START);
    timings.push(frames);
    let during = app.drag_stats;
    timings.sort_unstable();
    let per_frame_ms = timings[1].as_secs_f64() * 1_000.0 / 30.0;

    assert_eq!(during.frames, 30, "one applied frame per tick");
    assert_eq!(during.redraws, 30, "exactly one redraw per frame");
    assert_eq!(
        during.row_rebuilds, 0,
        "the live drag never rebuilds the row model"
    );
    assert!(
        frame_writes.iter().all(|writes| *writes <= 5),
        "each frame writes only the five dragged rows ({frame_writes:?})"
    );
    assert!(during.row_updates > 0, "the dragged rows actually moved");
    let dirty = u64::try_from(dirty).unwrap_or(u64::MAX);
    assert_eq!(
        during.wire_paths,
        30 * dirty,
        "only the {dirty} dirty wire paths were regenerated"
    );
    assert_eq!(during.wire_pushes, 30, "one wires string per frame");
    let range_ms = (
        timings[0].as_secs_f64() * 1_000.0 / 30.0,
        timings[2].as_secs_f64() * 1_000.0 / 30.0,
    );
    eprintln!(
        "node drag after (coalesced): {per_frame_ms:.3} ms/frame (median of 3, range {:.3}-{:.3}); {} row writes, {} dirty wire paths, {} wire pushes, {} redraws",
        range_ms.0,
        range_ms.1,
        during.row_updates,
        during.wire_paths,
        during.wire_pushes,
        during.redraws,
    );
    assert!(
        per_frame_ms < DRAG_FRAME_BUDGET_MS,
        "a coalesced node-drag frame stays under {DRAG_FRAME_BUDGET_MS} ms in the dev profile ({per_frame_ms:.3} ms)"
    );

    // Release: the exact release point, then one full sync (the one rebuild).
    let release = (DRAG_START.0 + 240.25, DRAG_START.1 + 120.125);
    release_drag(&mut app, &ui, &mut clock, DRAG_START);
    let total = app.drag_stats;
    assert_eq!(
        total.row_rebuilds, 1,
        "the release is the one full row rebuild of the gesture"
    );
    assert_eq!(total.frames, 30, "the release is not another tick");
    assert_eq!(total.redraws, 30, "the drag frames' redraws stay at 30");
    assert!(
        app.pending_drag.to.is_none(),
        "the release consumed the input"
    );
    assert_eq!(
        app.doc.selection().len(),
        DRAG_IDS.len(),
        "multi-select intact"
    );

    let mut expected = crate::doc::Doc::assemble(initial_nodes, initial_connections);
    for id in DRAG_IDS {
        expected.select_node(id, true);
    }
    expected.begin_node_drag(DRAG_IDS[0], ScreenPos::new(DRAG_START.0, DRAG_START.1));
    expected.node_drag_to(ScreenPos::new(release.0, release.1));
    for node in expected.nodes() {
        assert_eq!(
            app.doc.node(&node.id).map(|live| live.position),
            Some(node.position),
            "{} ends exactly where the reference drag lands",
            node.id
        );
    }
    Ok(())
}

/// A coalesced pan frame on a busy board: the tick applies the latest pointer,
/// runs the hot tier and (every other tick) the follow tier. Budget: under 2 ms
/// in the dev profile, i.e. eight such frames fit one 16 ms display frame.
#[test]
fn a_coalesced_pan_frame_stays_in_budget() -> TestResult {
    crate::test_support::init_backend_allow_existing();
    let (mut app, _dir) = fixture_app(200)?;
    let ui = crate::AppWindow::new()?;
    app.sync_view(&ui);
    let mut clock = 1_000u64;
    let mut pos = (600.0, 300.0);
    app.doc
        .begin_pan(crate::coords::ScreenPos::new(pos.0, pos.1));
    let time = median(3, || {
        for _ in 0..60 {
            for _ in 0..8 {
                pos.0 -= 1.5;
                clock += 1;
                app.clock_override = Some(clock);
                app.note_pan_at(crate::coords::ScreenPos::new(pos.0, pos.1), clock);
            }
            clock += 1;
            app.clock_override = Some(clock);
            app.view_tick_at(&ui, clock);
        }
    });
    let per_frame = time.as_secs_f64() * 1000.0 / 60.0;
    eprintln!("coalesced pan frame: {per_frame:.3} ms/frame (dev profile, 200 nodes / 400 wires)");
    assert!(
        per_frame < 2.0,
        "a coalesced pan frame stays under 2 ms in the dev profile ({per_frame:.3} ms)"
    );
    Ok(())
}

/// The 120 fps wheel-zoom contract on a busy board: ten wheel events arrive in
/// every 8 ms frame for 60 frames. The frame tick coalesces each batch into one
/// cursor-anchored zoom application, runs **only** the hot tier (scalars + one
/// redraw), and pays for the rulers once, in the cold pass after the gesture.
/// Budget: under 3 ms per frame in the dev profile.
#[test]
fn a_coalesced_wheel_zoom_frame_stays_in_budget() -> TestResult {
    crate::test_support::init_backend_allow_existing();
    let (mut app, _dir) = fixture_app(200)?;
    let ui = crate::AppWindow::new()?;
    app.sync_view(&ui);
    let tile_before = app.grid_tile.as_ref().map(|tile| tile.key);
    assert!(tile_before.is_some(), "the cold sync built the tile");
    let cursor = ScreenPos::new(640.0, 360.0);
    let mut clock = 20_000u64;
    let mut timings: Vec<Duration> = Vec::with_capacity(3);
    let mut stats = crate::app::ViewStats::default();
    for run in 0..3 {
        app.doc.viewport = crate::coords::Viewport::new();
        app.pending = crate::app::PendingView::default();
        app.clock_override = Some(clock);
        app.view_stats.reset();
        let before = app.doc.viewport().screen_to_world(cursor);
        let started = Instant::now();
        for _ in 0..60 {
            for _ in 0..10 {
                app.clock_override = Some(clock);
                app.note_wheel_at(cursor, 1.0, clock);
            }
            clock += crate::FRAME_TICK_MS;
            app.clock_override = Some(clock);
            app.view_tick_at(&ui, clock);
        }
        // The gesture ended: the one cold pass lands after the settle window.
        clock += crate::sync::view::VIEW_COLD_SETTLE_MS;
        app.clock_override = Some(clock);
        app.view_tick_at(&ui, clock);
        timings.push(started.elapsed());
        if run == 2 {
            stats = app.view_stats;
            let after = app.doc.viewport().screen_to_world(cursor);
            assert!(
                (after.x - before.x).abs() < 1e-9 && (after.y - before.y).abs() < 1e-9,
                "the whole burst stays anchored under the cursor: {before:?} -> {after:?}"
            );
        }
    }
    timings.sort_unstable();
    let per_frame = timings[1].as_secs_f64() * 1000.0 / 60.0;
    eprintln!(
        "coalesced wheel zoom: {per_frame:.3} ms/frame, {} hot applications, {} redraws, {} follow passes, {} cold, {} ruler passes (dev profile, 200 nodes / 400 wires)",
        stats.applications,
        stats.redraws,
        stats.follow_passes,
        stats.cold_passes,
        stats.ruler_passes,
    );
    assert!(
        stats.applications <= 60,
        "at most one application per frame ({} for 600 events)",
        stats.applications
    );
    assert_eq!(
        stats.hot_passes, stats.applications,
        "every applied frame runs exactly one hot pass"
    );
    assert!(
        stats.redraws <= 60,
        "at most one redraw per frame ({} for 600 events)",
        stats.redraws
    );
    assert_eq!(
        stats.cold_passes, 1,
        "the ruler/panel layers sync once, after the gesture"
    );
    assert_eq!(
        stats.ruler_passes, 1,
        "ruler ticks/labels are deferred to the one cold pass"
    );
    assert_eq!(
        stats.follow_passes, 0,
        "a wheel burst runs only the hot tier per frame"
    );
    assert!(
        per_frame < 3.0,
        "a coalesced wheel-zoom frame stays under 3 ms in the dev profile ({per_frame:.3} ms)"
    );
    assert_eq!(
        app.grid_tile.as_ref().map(|tile| tile.key),
        tile_before,
        "a zoom burst never re-rasterises the screen-constant dot tile"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Image Studio hot paths (restored: pre-rendered board, no per-frame composite)
// ---------------------------------------------------------------------------

/// A 1024x768 board with four painted layers and the storage keys the
/// compositor resolves them by.
fn board_fixture() -> (
    open_canvas_ps::ImageProject,
    std::collections::HashMap<String, open_canvas_ps::Surface>,
) {
    let mut project = crate::image::ImageState::default().project;
    project.id = "img-perf".to_string();
    project.title = "Perf board".to_string();
    project.width = 1024.0;
    project.height = 768.0;
    let mut gen = open_canvas_ps::IdGen::new("perf");
    let mut layers = Vec::new();
    let mut bitmaps = std::collections::HashMap::new();
    // Four visibly different painted areas, one tone per layer.
    let tones: [[u8; 4]; 4] = [
        [60, 120, 200, 200],
        [100, 120, 170, 200],
        [140, 120, 140, 200],
        [180, 120, 110, 200],
    ];
    for (index, tone) in tones.iter().enumerate() {
        let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (1024.0, 768.0), "Layer");
        let key = format!("image:perf-{index}");
        layer.storage_key = Some(key.clone());
        let mut surface = open_canvas_ps::Surface::new(1024, 768);
        for y in 0..768 {
            for x in 0..1024 {
                // A diagonal ramp keeps each layer's pixels distinct (so a
                // stale cache cannot accidentally compare equal).
                let lift = u8::try_from((x + y) % 16).unwrap_or(0);
                surface.set(
                    coord(x),
                    coord(y),
                    [
                        tone[0].saturating_add(lift),
                        tone[1],
                        tone[2].saturating_sub(lift),
                        tone[3],
                    ],
                );
            }
        }
        bitmaps.insert(key, surface);
        layers.push(layer);
    }
    project.board_layers = layers;
    (project, bitmaps)
}

/// One uncached 1024x768 composite over four layers: the reference cost the
/// display cache exists to avoid paying per frame.
#[test]
#[expect(
    clippy::unnecessary_wraps,
    reason = "every perf test shares the `TestResult` signature so a future `?` stays local"
)]
fn image_display_1024x768_4_layers() -> TestResult {
    let (project, bitmaps) = board_fixture();
    assert!(!bitmaps.is_empty(), "the fixture paints the board");
    let time = median(3, || {
        std::hint::black_box(crate::image::render::board_surface(&project, &bitmaps));
    });
    report("one 1024x768 board composite (4 layers)", time);
    assert!(
        time < Duration::from_secs(30),
        "one full board composite stays under 30 s in the dev profile ({time:?})"
    );
    Ok(())
}

/// 60 studio syncs whose inputs did not change: the cache must answer in
/// microseconds, not with another composite.
#[test]
fn image_display_cached_60_syncs() -> TestResult {
    let (project, bitmaps) = board_fixture();
    let (mut app, _dir) = crate::test_support::app_with_image(project, bitmaps)?;
    assert!(
        app.image
            .cached_display()
            .is_some_and(|(_, changed)| changed),
        "the first display is a composite"
    );
    let time = median(3, || {
        for _ in 0..60 {
            std::hint::black_box(app.image.cached_display().map(|(_, changed)| changed));
        }
    });
    report("60 unchanged studio displays (1024x768, 4 layers)", time);
    assert!(
        time < Duration::from_millis(500),
        "60 unchanged displays cost almost nothing ({time:?})"
    );
    Ok(())
}

/// The cache is exact: it holds what the uncached composite would produce, and
/// one written pixel invalidates it.
#[test]
fn cached_display_matches_the_uncached_composite() -> TestResult {
    let (project, bitmaps) = board_fixture();
    let (mut app, _dir) = crate::test_support::app_with_image(project, bitmaps)?;
    let cached = app
        .image
        .cached_display()
        .map(|(surface, _)| surface.data().to_vec());
    let uncached =
        crate::image::render::display_of(&app.image).map(|surface| surface.data().to_vec());
    assert_eq!(cached, uncached, "the cache holds the composite");

    let layer = app.image.active_layer_id();
    let key = app
        .image
        .ensure_layer_bitmap(&layer)
        .ok_or("the fixture layer has a bitmap")?;
    app.image
        .bitmaps
        .get_mut(&key)
        .ok_or("the bitmap exists")?
        .set(0, 0, [1, 2, 3, 255]);
    assert_eq!(
        app.image.cached_display().map(|(_, changed)| changed),
        Some(true),
        "one written pixel invalidates the cache"
    );
    let cached = app
        .image
        .cached_display()
        .map(|(surface, _)| surface.data().to_vec());
    let uncached =
        crate::image::render::display_of(&app.image).map(|surface| surface.data().to_vec());
    assert_eq!(cached, uncached, "the rebuilt cache is still exact");
    Ok(())
}

/// 60 studio syncs while the board is panned: no composite, no 3 MB image copy.
#[test]
fn sync_image_60_pans() -> TestResult {
    crate::test_support::init_backend_allow_existing();
    let (project, bitmaps) = board_fixture();
    let (mut app, _dir) = crate::test_support::app_with_image(project, bitmaps)?;
    let ui = crate::AppWindow::new()?;
    let time = median(3, || {
        for _ in 0..60 {
            app.sync_image(&ui);
        }
    });
    report("60 studio syncs while panning (1024x768, 4 layers)", time);
    assert!(
        time < Duration::from_secs(10),
        "60 panned studio syncs stay under 10 s ({time:?})"
    );
    assert!(
        app.image.meta.is_some(),
        "the studio stayed on the open board"
    );
    Ok(())
}

/// 20 brush strokes including their undo history: one pre-edit clone and one
/// project clone per stroke, no per-event copy.
#[test]
fn image_history_stroke_20() -> TestResult {
    let (project, bitmaps) = board_fixture();
    let (mut app, _dir) = crate::test_support::app_with_image(project, bitmaps)?;
    let layer = app.image.active_layer_id();
    let key = app
        .image
        .ensure_layer_bitmap(&layer)
        .ok_or("the fixture layer has a bitmap")?;
    let time = median(3, || {
        for step in 0..20u32 {
            app.image.begin_edit(std::slice::from_ref(&key));
            if let Some(bitmap) = app.image.bitmaps.get_mut(&key) {
                let offset = usize::try_from(step % 64).unwrap_or(0);
                bitmap.set(coord(offset), coord(offset), [255, 0, 0, 255]);
            }
            app.image.commit("stroke");
        }
    });
    report("20 brush strokes incl. history (1024x768)", time);
    assert!(
        time < Duration::from_secs(5),
        "20 strokes stay under 5 s ({time:?})"
    );
    Ok(())
}

/// 10 filter-preview slider ticks: every tick re-runs the chosen filter on the
/// full-resolution layer and downscales the result for the 320 px preview.
#[test]
fn filter_preview_slider_ticks() -> TestResult {
    let (project, bitmaps) = board_fixture();
    let (mut app, _dir) = crate::test_support::app_with_image(project, bitmaps)?;
    app.image_filter_open_core("gaussian-blur")?;
    assert!(
        app.image.filter_dialog.is_some(),
        "the filter dialog opened on the painted layer"
    );
    let time = median(3, || {
        for step in 0..10u32 {
            app.image_filter_param_core("radius", 2.0 + f64::from(step) * 4.0);
        }
    });
    report("10 filter-preview slider ticks (1024x768 layer)", time);
    assert!(
        time < Duration::from_secs(60),
        "10 filter previews stay under 60 s in the dev profile ({time:?})"
    );
    Ok(())
}

/// Local profiling hook: drag one real attempt card from the user's project and
/// report the per-frame Rust work. Ignored by default; run with
/// `cargo test -p open-canvas-app drag_actual_project -- --ignored --nocapture`.
#[test]
#[ignore = "local profile against the user's project"]
fn drag_actual_project_frames() -> TestResult {
    crate::test_support::init_backend_allow_existing();
    let ui = crate::AppWindow::new()?;
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA")?;
    let path = std::path::PathBuf::from(appdata)
        .join("open-canvas-desktop")
        .join("projects")
        .join("project-17295-2.json");
    let text = std::fs::read_to_string(path)?;
    let project: open_canvas_core::document::CanvasProject = serde_json::from_str(&text)?;
    let store = crate::store::Store::at(std::env::temp_dir());
    let mut app = App::new(store);
    app.doc = crate::doc::Doc::from_project(&project);
    app.sync_all(&ui);
    let id = app
        .doc
        .nodes()
        .iter()
        .find(|node| node.node_type == open_canvas_core::node::CanvasNodeType::ImageAttempt)
        .map(|node| node.id.clone())
        .ok_or("attempt")?;
    app.doc.select_node(&id, false);
    app.sync_rows_full(&ui);
    app.clock_override = Some(10_000);
    let start = ScreenPos::new(500.0, 400.0);
    app.node_pointer(&ui, &id, &drag_pointer(PointerKind::Down, start.x, start.y));
    let mut clock = 10_000u64;
    let mut pos = start;
    let mut times = Vec::new();
    for _ in 0..30 {
        for _ in 0..8 {
            pos.x += 1.0;
            pos.y += 0.5;
            clock += 1;
            app.clock_override = Some(clock);
            app.node_pointer(&ui, &id, &drag_pointer(PointerKind::Move, pos.x, pos.y));
        }
        clock += 1;
        app.clock_override = Some(clock);
        let t = Instant::now();
        app.node_drag_tick(&ui);
        times.push(t.elapsed());
    }
    let total: Duration = times.iter().sum();
    let max = times.iter().max().copied().unwrap_or_default();
    eprintln!(
        "actual project drag: {:.3} ms/frame avg, {:.3} ms max, {} nodes",
        total.as_secs_f64() * 1_000.0 / 30.0,
        max.as_secs_f64() * 1_000.0,
        app.doc.nodes().len()
    );
    app.doc.end_node_drag();
    Ok(())
}
