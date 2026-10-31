//! End-to-end wiring tests for the Image Studio tool rail.
//!
//! These drive the *real* entry points the Slint UI calls: `App::image_tool`
//! (the rail's `tool-pick`) and `App::image_pointer` (the board's
//! `canvas-pointer`), plus the actual rail buttons through the headless
//! testing backend. The table then asserts one observable document change per
//! tool id, so an id that stops being handled cannot pass silently.
//!
//! Root cause these tests guard: the studio's keyboard `FocusScope` spans the
//! whole workspace and `focus-on-click` (the default) made it swallow every
//! pointer press before the rail/board/menus saw it.

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model as _};

use crate::app::App;
use crate::image::{ImageTool, ImageView, TOOLS};
use crate::input::{Button, PointerKind};
use crate::store::Store;
use crate::test_support::init_backend_allow_existing;
use crate::AppWindow;

/// The document is 32x32 and pinned at 4 screen px per document px with no
/// offset, so a document point maps to `(x * K, y * K)` inside the board.
const DOC: f64 = 32.0;
const K: f64 = 4.0;

type Harness = (AppWindow, Rc<RefCell<App>>, tempfile::TempDir);

fn harness() -> Result<Harness, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(App::new(store)));
    crate::events::wire(&ui, &app);

    let mut project = crate::image::ImageState::default().project;
    project.id = "img-tool".to_string();
    project.title = "Tool test".to_string();
    project.width = DOC;
    project.height = DOC;
    let mut gen = open_canvas_ps::IdGen::new("tool");
    let key = "image:tool-1".to_string();
    let mut layer = open_canvas_ps::create_ps_pixel_layer(&mut gen, (12.0, 12.0), "Layer 1");
    layer.x = 4.0;
    layer.y = 4.0;
    layer.storage_key = Some(key.clone());
    project.board_layers = vec![layer];
    let mut bitmaps = std::collections::HashMap::new();
    bitmaps.insert(key, open_canvas_ps::Surface::new(12, 12));
    crate::app::with_app(&app, |app| {
        app.enter_image(
            project,
            bitmaps,
            crate::image::assets::ImageAssets::default(),
        );
        app.sync_all(&ui);
        // `sync_image` auto-fits once per project+size key; pin the key so the
        // deterministic 4x view below survives every later sync.
        app.image.canvas = (400.0, 400.0);
        app.image.fitted = format!("img-tool:{DOC}x{DOC}");
        app.image.view = ImageView {
            x: 0.0,
            y: 0.0,
            k: K,
        };
    });
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    Ok((ui, app, dir))
}

fn at(doc_x: f64, doc_y: f64) -> (f64, f64) {
    (doc_x * K, doc_y * K)
}

fn click(ui: &AppWindow, x: f32, y: f32) {
    let window = ui.window();
    let pos = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position: pos });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: pos,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: pos,
        button: PointerEventButton::Left,
    });
}

/// One distinct press/release at a document point (polygon-lasso vertices).
fn tap(app: &mut App, ui: &AppWindow, doc: (f64, f64)) {
    let (x, y) = at(doc.0, doc.1);
    app.image_pointer(ui, PointerKind::Down, x, y, Button::Left, false, false);
    app.image_pointer(ui, PointerKind::Up, x, y, Button::Left, false, false);
}

fn selection_pixels(app: &App) -> usize {
    app.image.selection.as_ref().map_or(0, |mask| {
        let mut count = 0;
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                if mask.get(crate::vm::u2i(x), crate::vm::u2i(y))[3] > 0 {
                    count += 1;
                }
            }
        }
        count
    })
}

fn active_bitmap(app: &App) -> Option<&open_canvas_ps::Surface> {
    let id = app.image.active_layer_id();
    let layer = app
        .image
        .project
        .board_layers
        .iter()
        .find(|layer| layer.id == id)?;
    app.image.bitmaps.get(layer.storage_key.as_deref()?)
}

fn last_label(app: &App) -> Option<String> {
    app.image
        .history
        .stack
        .last()
        .map(|snap| snap.label.clone())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

type Check = fn(&mut App, &AppWindow) -> Result<(), String>;

/// Hand selects and drags an object; a press off the object does not pan.
fn check_hand(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    let before = app.image.project.board_layers[0].clone();
    select(app, ui, "hand");
    drag(app, ui, &[at(8.0, 8.0), at(14.0, 12.0)]);
    let after = &app.image.project.board_layers[0];
    if !near(after.x, before.x + 6.0) || !near(after.y, before.y + 4.0) {
        return Err(format!(
            "hand put the layer at ({}, {}), expected ({}, {})",
            after.x,
            after.y,
            before.x + 6.0,
            before.y + 4.0
        ));
    }

    let view = app.image.view;
    drag(app, ui, &[at(24.0, 24.0), at(28.0, 28.0)]);
    if !near(app.image.view.x, view.x) || !near(app.image.view.y, view.y) {
        return Err(format!(
            "hand off the layer must not pan the view ({:?})",
            app.image.view
        ));
    }
    Ok(())
}

fn check_pen(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "pen");
    drag(app, ui, &[at(5.0, 5.0), at(8.0, 8.0)]);
    let path = app
        .image
        .project
        .board_paths
        .as_ref()
        .and_then(|paths| paths.first())
        .ok_or("pen created no path")?;
    let anchor = path.anchors.first().ok_or("pen created no anchor")?;
    if !near(anchor.x, 5.0) || !near(anchor.handle_out.x, 3.0) {
        return Err(format!("pen anchor did not follow the drag: {anchor:?}"));
    }
    Ok(())
}

fn check_direct_select(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    let path = open_canvas_ps::create_path(
        "path-1",
        "Path",
        vec![open_canvas_ps::path_anchor(8.0, 8.0)],
        false,
    );
    app.image.project.board_paths = Some(vec![path]);
    app.image.active_path = "path-1".to_string();
    select(app, ui, "direct-select");
    drag(app, ui, &[at(8.0, 8.0), at(12.0, 12.0)]);
    let anchor = app
        .image
        .project
        .board_paths
        .as_ref()
        .and_then(|paths| paths.first())
        .and_then(|path| path.anchors.first())
        .ok_or("direct-select lost the anchor")?;
    if !near(anchor.x, 12.0) || !near(anchor.y, 12.0) {
        return Err(format!("direct-select did not move the anchor: {anchor:?}"));
    }
    Ok(())
}

/// A direct-select click can grab an anchor on a path that is not active;
/// activating that path and moving the anchor happen in one gesture.
#[test]
fn direct_select_grabs_an_anchor_on_an_inactive_path() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.project.board_paths = Some(vec![
            open_canvas_ps::create_path(
                "path-1",
                "Path 1",
                vec![
                    open_canvas_ps::path_anchor(2.0, 2.0),
                    open_canvas_ps::path_anchor(6.0, 2.0),
                ],
                false,
            ),
            open_canvas_ps::create_path(
                "path-2",
                "Path 2",
                vec![
                    open_canvas_ps::path_anchor(20.0, 20.0),
                    open_canvas_ps::path_anchor(28.0, 20.0),
                ],
                false,
            ),
        ]);
        app.image.active_path = "path-1".to_string();
        app.image_tool(&ui, "direct-select");
        drag(app, &ui, &[at(20.0, 20.0), at(24.0, 22.0)]);
    });
    let (active, anchor) = crate::app::with_app_r(&app, |app| {
        let anchor = app
            .image
            .project
            .board_paths
            .as_ref()
            .and_then(|paths| paths.iter().find(|path| path.id == "path-2"))
            .and_then(|path| path.anchors.first())
            .copied();
        (app.image.active_path.clone(), anchor)
    })
    .ok_or("borrowed")?;
    let anchor = anchor.ok_or("path-2 anchor missing")?;
    assert_eq!(active, "path-2", "the clicked path becomes active");
    assert!(
        near(anchor.x, 24.0) && near(anchor.y, 22.0),
        "the inactive path's anchor follows the drag: {anchor:?}"
    );
    Ok(())
}

/// Clicking a path's stroke with Direct Select activates it, so its anchor
/// overlay appears without first finding the Paths panel.
#[test]
fn direct_select_clicking_a_stroke_activates_its_path() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.project.board_paths = Some(vec![
            open_canvas_ps::create_path(
                "path-1",
                "Path 1",
                vec![
                    open_canvas_ps::path_anchor(2.0, 2.0),
                    open_canvas_ps::path_anchor(6.0, 2.0),
                ],
                false,
            ),
            open_canvas_ps::create_path(
                "path-2",
                "Path 2",
                vec![
                    open_canvas_ps::path_anchor(4.0, 20.0),
                    open_canvas_ps::path_anchor(28.0, 20.0),
                ],
                false,
            ),
        ]);
        app.image.active_path = "path-1".to_string();
        app.image_tool(&ui, "direct-select");
        tap(app, &ui, (16.0, 21.0));
    });
    let active =
        crate::app::with_app_r(&app, |app| app.image.active_path.clone()).ok_or("borrowed")?;
    assert_eq!(active, "path-2", "the stroke click selects the path");
    Ok(())
}

/// The Pen tool selects an existing anchor/stroke instead of silently adding
/// another anchor on top of it.
#[test]
fn pen_clicking_an_existing_anchor_or_stroke_selects_it() -> Result<(), Box<dyn std::error::Error>>
{
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.project.board_paths = Some(vec![
            open_canvas_ps::create_path(
                "path-1",
                "Path 1",
                vec![
                    open_canvas_ps::path_anchor(2.0, 2.0),
                    open_canvas_ps::path_anchor(6.0, 2.0),
                ],
                false,
            ),
            open_canvas_ps::create_path(
                "path-2",
                "Path 2",
                vec![
                    open_canvas_ps::path_anchor(4.0, 20.0),
                    open_canvas_ps::path_anchor(28.0, 20.0),
                ],
                false,
            ),
        ]);
        app.image.active_path = "path-1".to_string();
        app.image_tool(&ui, "pen");
        tap(app, &ui, (4.0, 20.0));
    });
    let (active, anchors) = crate::app::with_app_r(&app, |app| {
        let anchors = app
            .image
            .project
            .board_paths
            .as_ref()
            .and_then(|paths| paths.iter().find(|path| path.id == "path-2"))
            .map_or(0, |path| path.anchors.len());
        (app.image.active_path.clone(), anchors)
    })
    .ok_or("borrowed")?;
    assert_eq!(active, "path-2", "the pen selects the clicked path");
    assert_eq!(anchors, 2, "selecting an anchor never appends a duplicate");

    crate::app::with_app(&app, |app| {
        app.image_tool(&ui, "pen");
        tap(app, &ui, (16.0, 20.0));
    });
    let (active, anchors) = crate::app::with_app_r(&app, |app| {
        let anchors = app
            .image
            .project
            .board_paths
            .as_ref()
            .and_then(|paths| paths.iter().find(|path| path.id == "path-2"))
            .map_or(0, |path| path.anchors.len());
        (app.image.active_path.clone(), anchors)
    })
    .ok_or("borrowed")?;
    assert_eq!(active, "path-2", "the pen selects the clicked stroke");
    assert_eq!(anchors, 2, "clicking the stroke does not append an anchor");
    Ok(())
}

fn check_marquee(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "marquee");
    drag(app, ui, &[at(2.0, 2.0), at(10.0, 8.0)]);
    let mask = app
        .image
        .selection
        .as_ref()
        .ok_or("marquee set no selection")?;
    if mask.get(4, 4)[3] == 0 || mask.get(20, 20)[3] != 0 {
        return Err("marquee selection does not match its drag box".to_string());
    }
    Ok(())
}

fn check_ellipse_marquee(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "ellipse-marquee");
    drag(app, ui, &[at(2.0, 2.0), at(10.0, 10.0)]);
    let mask = app
        .image
        .selection
        .as_ref()
        .ok_or("ellipse marquee set no selection")?;
    if mask.get(6, 6)[3] == 0 || mask.get(2, 2)[3] != 0 {
        return Err("ellipse marquee is not elliptical".to_string());
    }
    Ok(())
}

fn check_lasso(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "lasso");
    drag(app, ui, &[at(2.0, 2.0), at(10.0, 2.0), at(10.0, 10.0)]);
    if selection_pixels(app) == 0 {
        return Err("lasso set no selection".to_string());
    }
    Ok(())
}

fn check_polygon_lasso(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "polygon-lasso");
    tap(app, ui, (2.0, 2.0));
    tap(app, ui, (10.0, 2.0));
    tap(app, ui, (10.0, 10.0));
    let (x, y) = at(10.0, 10.0);
    app.image_canvas_double_click(ui, x, y);
    if selection_pixels(app) == 0 {
        return Err("polygon lasso set no selection".to_string());
    }
    Ok(())
}

fn check_wand(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "wand");
    click_doc(app, ui, (10.0, 10.0));
    if selection_pixels(app) == 0 {
        return Err("magic wand set no selection".to_string());
    }
    Ok(())
}

fn check_quick_select(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "quick-select");
    click_doc(app, ui, (10.0, 10.0));
    if selection_pixels(app) == 0 {
        return Err("quick select set no selection".to_string());
    }
    Ok(())
}

fn check_brush(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    app.image.color = "#ff0000".to_string();
    app.image.brush_size = 6.0;
    select(app, ui, "brush");
    drag(app, ui, &[at(8.0, 8.0), at(13.0, 8.0)]);
    let bitmap = active_bitmap(app).ok_or("brush painted no bitmap")?;
    let dab = bitmap.get(4, 4);
    if dab[3] == 0 || dab[0] < 200 {
        return Err(format!("brush left the layer pixel {dab:?}"));
    }
    Ok(())
}

fn check_eraser(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    if let Some(layer) = app.image.project.board_layers.first() {
        if let Some(key) = layer.storage_key.clone() {
            if let Some(bitmap) = app.image.bitmaps.get_mut(&key) {
                for y in 0..bitmap.height() {
                    for x in 0..bitmap.width() {
                        bitmap.set(crate::vm::u2i(x), crate::vm::u2i(y), [255, 0, 0, 255]);
                    }
                }
            }
        }
    }
    select(app, ui, "eraser");
    drag(app, ui, &[at(8.0, 8.0), at(12.0, 8.0)]);
    let bitmap = active_bitmap(app).ok_or("eraser lost the bitmap")?;
    if bitmap.get(6, 4)[3] != 0 {
        return Err(format!(
            "eraser left alpha {} at the drag centre",
            bitmap.get(6, 4)[3]
        ));
    }
    Ok(())
}

fn check_bucket(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    app.image.color = "#00ff00".to_string();
    select(app, ui, "bucket");
    click_doc(app, ui, (10.0, 10.0));
    let bitmap = active_bitmap(app).ok_or("bucket painted no bitmap")?;
    let filled = bitmap.get(6, 6);
    if filled[3] == 0 || filled[1] < 200 {
        return Err(format!("bucket left the layer pixel {filled:?}"));
    }
    Ok(())
}

fn check_gradient(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    select(app, ui, "gradient");
    drag(app, ui, &[at(5.0, 5.0), at(15.0, 15.0)]);
    let bitmap = active_bitmap(app).ok_or("gradient painted no bitmap")?;
    let start = bitmap.get(1, 1);
    let end = bitmap.get(11, 11);
    if start == end || start[3] == 0 || end[3] == 0 {
        return Err(format!(
            "gradient did not ramp across the drag: {start:?} vs {end:?}"
        ));
    }
    Ok(())
}

fn check_shape(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    let before = app.image.project.board_layers.len();
    select(app, ui, "shape");
    drag(app, ui, &[at(4.0, 4.0), at(16.0, 16.0)]);
    if app.image.project.board_layers.len() != before + 1 {
        return Err("shape created no layer".to_string());
    }
    let layer = app
        .image
        .project
        .board_layers
        .last()
        .ok_or("shape created no layer")?;
    if layer.kind != open_canvas_ps::CanvasPsLayerKind::Shape {
        return Err(format!("shape created a {:?} layer", layer.kind));
    }
    Ok(())
}

fn check_eyedropper(app: &mut App, ui: &AppWindow) -> Result<(), String> {
    if let Some(layer) = app.image.project.board_layers.first() {
        if let Some(key) = layer.storage_key.clone() {
            if let Some(bitmap) = app.image.bitmaps.get_mut(&key) {
                bitmap.set(0, 0, [255, 0, 0, 255]);
            }
        }
    }
    app.image.color = "#000000".to_string();
    let expected = app
        .sample_board_color(open_canvas_ps::Point::new(4.0, 4.0))
        .ok_or("the board did not composite")?;
    select(app, ui, "eyedropper");
    click_doc(app, ui, (4.5, 4.5));
    if app.image.color != expected {
        return Err(format!(
            "eyedropper sampled {} instead of {}",
            app.image.color, expected
        ));
    }
    Ok(())
}

fn select(app: &mut App, ui: &AppWindow, tool: &str) {
    app.image_tool(ui, tool);
}

fn drag(app: &mut App, ui: &AppWindow, points: &[(f64, f64)]) {
    let Some(first) = points.first() else {
        return;
    };
    app.image_pointer(
        ui,
        PointerKind::Down,
        first.0,
        first.1,
        Button::Left,
        false,
        false,
    );
    for point in &points[1..] {
        app.image_pointer(
            ui,
            PointerKind::Move,
            point.0,
            point.1,
            Button::Left,
            false,
            false,
        );
    }
    if let Some(last) = points.last() {
        app.image_pointer(
            ui,
            PointerKind::Up,
            last.0,
            last.1,
            Button::Left,
            false,
            false,
        );
    }
}

fn click_doc(app: &mut App, ui: &AppWindow, doc: (f64, f64)) {
    let (x, y) = at(doc.0, doc.1);
    app.image_pointer(ui, PointerKind::Down, x, y, Button::Left, false, false);
    app.image_pointer(ui, PointerKind::Up, x, y, Button::Left, false, false);
}

/// The per-tool table: every id the rail can emit maps to a real behaviour.
#[test]
fn every_tool_changes_the_document() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let cases: &[(&str, Check)] = &[
        ("hand", check_hand),
        ("pen", check_pen),
        ("direct-select", check_direct_select),
        ("marquee", check_marquee),
        ("ellipse-marquee", check_ellipse_marquee),
        ("lasso", check_lasso),
        ("polygon-lasso", check_polygon_lasso),
        ("wand", check_wand),
        ("quick-select", check_quick_select),
        ("brush", check_brush),
        ("eraser", check_eraser),
        ("bucket", check_bucket),
        ("gradient", check_gradient),
        ("shape", check_shape),
        ("eyedropper", check_eyedropper),
    ];
    assert_eq!(
        cases.len(),
        TOOLS.len(),
        "the table must cover every rail tool id"
    );
    for (id, check) in cases {
        let (ui, app, _dir) = harness()?;
        crate::app::with_app(&app, |app| app.image_tool(&ui, id));
        let outcome = crate::app::with_app_r(&app, |app| check(app, &ui))
            .ok_or("the app was borrowed by a callback")?;
        outcome.map_err(|message| format!("{id}: {message}"))?;
    }
    Ok(())
}

/// The tool rail is a one-click target even while a menu is open: the menu's
/// click-away layer must not swallow the first press on a tool button.
#[test]
fn tool_rail_click_while_a_menu_is_open_selects_in_one_click(
) -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.menu = "file".to_string();
        app.sync_image(&ui);
    });
    // Pen is the second 24px rail button: menu bar 40 + rail top 4 + hand 24
    // + spacing 1, so its centre is at y=81.
    click(&ui, 20.0, 81.0);
    let (tool, menu) = crate::app::with_app_r(&app, |app| (app.image.tool, app.image.menu.clone()))
        .ok_or("borrowed")?;
    assert_eq!(tool, ImageTool::Pen, "the first rail click picks Pen");
    assert!(menu.is_empty(), "the same click dismisses the open menu");
    Ok(())
}

/// A single brush click is a one-dab stroke; it must not require a drag before
/// the first paint appears.
#[test]
fn brush_single_click_paints_a_dab() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.color = "#ff0000".to_string();
        app.image.brush_size = 6.0;
        app.image_tool(&ui, "brush");
        click_doc(app, &ui, (8.0, 8.0));
    });
    let dab = crate::app::with_app_r(&app, |app| {
        active_bitmap(app).map(|bitmap| bitmap.get(4, 4))
    })
    .ok_or("borrowed")?
    .ok_or("brush painted no bitmap")?;
    assert!(
        dab[3] > 0 && dab[0] > 200,
        "the click leaves a red dab: {dab:?}"
    );
    Ok(())
}

/// The real Slint board hit area (not the Rust gesture core) must deliver a
/// brush drag. This exercises the same window coordinates a user clicks.
#[test]
fn brush_drag_through_the_board_hit_area_paints() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.project.width = 640.0;
        app.image.project.height = 360.0;
        app.image.project.board_layers.clear();
        app.image.active_layer.clear();
        app.image.canvas = (1240.0, 760.0);
        app.image.fitted = "img-tool:640x360".to_string();
        app.image.view = ImageView {
            x: 100.0,
            y: 100.0,
            k: 1.0,
        };
        app.image.color = "#ff0000".to_string();
        app.image.brush_size = 12.0;
        app.image_tool(&ui, "brush");
    });
    // Board-local (500, 400) = document (400, 300), outside the floating
    // options card; window = menu bar 40 + rail 40 + board-local.
    let down = slint::LogicalPosition::new(540.0, 440.0);
    let moved = slint::LogicalPosition::new(560.0, 440.0);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: down });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: down,
        button: PointerEventButton::Left,
    });
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: moved });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: moved,
        button: PointerEventButton::Left,
    });
    let (layers, dab) = crate::app::with_app_r(&app, |app| {
        let layer = app
            .image
            .project
            .board_layers
            .iter()
            .find(|layer| layer.id == app.image.active_layer_id())
            .cloned();
        let dab = layer.and_then(|layer| layer.storage_key).and_then(|key| {
            app.image
                .bitmaps
                .get(&key)
                .map(|bitmap| bitmap.get(400, 300))
        });
        (app.image.project.board_layers.len(), dab)
    })
    .ok_or("borrowed")?;
    assert!(layers >= 1, "the brush creates a pixel layer");
    let dab = dab.ok_or("the brush created no bitmap")?;
    assert!(
        dab[3] > 0 && dab[0] > 200,
        "the brush stroke is visible: {dab:?}"
    );
    Ok(())
}

/// One history step per committed gesture, and no step for a pan.
#[test]
fn history_records_one_step_per_gesture() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    let before =
        crate::app::with_app_r(&app, |app| app.image.history.stack.len()).ok_or("borrowed")?;
    crate::app::with_app(&app, |app| {
        app.image.color = "#ff0000".to_string();
        app.image_tool(&ui, "brush");
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(8.0, 8.0).0,
            at(8.0, 8.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(12.0, 8.0).0,
            at(12.0, 8.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(12.0, 8.0).0,
            at(12.0, 8.0).1,
            Button::Left,
            false,
            false,
        );
    });
    let after =
        crate::app::with_app_r(&app, |app| (app.image.history.stack.len(), last_label(app)))
            .ok_or("borrowed")?;
    assert_eq!(after.0, before + 1, "the brush gesture is one history step");
    assert_eq!(after.1.as_deref(), Some("Brush"));

    let drags =
        crate::app::with_app_r(&app, |app| app.image.history.stack.len()).ok_or("borrowed")?;
    crate::app::with_app(&app, |app| {
        app.image_tool(&ui, "hand");
        // Off the 12x12 layer (4,4..16,16): this press pans, never moves.
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(24.0, 24.0).0,
            at(24.0, 24.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(28.0, 28.0).0,
            at(28.0, 28.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(28.0, 28.0).0,
            at(28.0, 28.0).1,
            Button::Left,
            false,
            false,
        );
    });
    let pans =
        crate::app::with_app_r(&app, |app| app.image.history.stack.len()).ok_or("borrowed")?;
    assert_eq!(pans, drags, "panning must not add history");
    Ok(())
}

/// Every rail id selects, and the rail's row model highlights exactly it.
#[test]
fn every_rail_id_selects_and_highlights() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    for (id, label, _) in TOOLS {
        crate::app::with_app(&app, |app| app.image_tool(&ui, id));
        assert_eq!(ui.get_image_active_tool().as_str(), id);
        let rows = ui.get_image_tools();
        let mut active = Vec::new();
        for row in rows.iter() {
            if row.active {
                active.push(row.id.to_string());
            }
        }
        assert_eq!(active, vec![id.to_string()], "rail highlight for {id}");
        assert!(
            rows.iter()
                .any(|row| row.id.as_str() == id && row.label.as_str() == label),
            "the rail row for {id} carries its label"
        );
    }
    Ok(())
}

/// The real buttons: one headless click per rail tool, then the two zoom
/// buttons in the status bar.
#[test]
fn rail_buttons_select_and_zoom_buttons_zoom() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;

    // The rail is one 24px button per row, 1px outer spacing, and each group
    // opens with a 1px separator plus its 1px inner spacing (see
    // `ImageToolRail::starts-group`). The studio is full-window: its 40px menu
    // bar + 4px rail padding put the first button at y=44.
    let mut y = 44.0f32;
    for (index, (id, _, _)) in TOOLS.iter().enumerate() {
        if index > 0 {
            y += 1.0;
            if matches!(*id, "pen" | "marquee" | "brush" | "shape" | "eyedropper") {
                y += 2.0;
            }
        }
        click(&ui, 20.0, y + 12.0);
        let selected = crate::app::with_app_r(&app, |app| app.image.tool.as_str().to_string())
            .ok_or("borrowed")?;
        assert_eq!(
            selected.as_str(),
            *id,
            "clicking the rail button at y={y} selected {selected}"
        );
        assert_eq!(ui.get_image_active_tool().as_str(), *id);
        y += 24.0;
    }

    // Status bar: zoom-out at x 8..24, zoom-in at x 52..68, y 780..796.
    let before = crate::app::with_app_r(&app, |app| app.image.view.k).ok_or("borrowed")?;
    click(&ui, 60.0, 788.0);
    let zoomed = crate::app::with_app_r(&app, |app| app.image.view.k).ok_or("borrowed")?;
    assert!(zoomed > before * 1.1, "zoom-in {before} -> {zoomed}");
    click(&ui, 16.0, 788.0);
    let back = crate::app::with_app_r(&app, |app| app.image.view.k).ok_or("borrowed")?;
    assert!(back < zoomed, "zoom-out {zoomed} -> {back}");
    Ok(())
}

/// Image > Crop to Selection, on top of a marquee selection.
#[test]
fn crop_to_selection_shrinks_the_board() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image_tool(&ui, "marquee");
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(4.0, 4.0).0,
            at(4.0, 4.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(20.0, 20.0).0,
            at(20.0, 20.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(20.0, 20.0).0,
            at(20.0, 20.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_action(&ui, "image.crop");
    });
    let (width, height) = crate::app::with_app_r(&app, |app| {
        (app.image.project.width, app.image.project.height)
    })
    .ok_or("borrowed")?;
    assert!(
        (width - 16.0).abs() <= 1.0 && (height - 16.0).abs() <= 1.0,
        "crop left {width}x{height}, expected 16x16"
    );
    Ok(())
}

/// The text layer's double-click editor still opens over a text layer.
#[test]
fn double_click_opens_the_text_editor() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| app.image_layer_action(&ui, "add-text", ""));
    let (x, y) = crate::app::with_app_r(&app, |app| {
        let layer = app.image.project.board_layers.last()?;
        let box_ = open_canvas_ps::layer_box(&app.image.project.board_layers, layer);
        let view = app.image.view;
        Some((
            view.x + (box_.x + box_.width / 2.0) * view.k,
            view.y + (box_.y + box_.height / 2.0) * view.k,
        ))
    })
    .flatten()
    .ok_or("add-text created no layer")?;
    crate::app::with_app(&app, |app| app.image_canvas_double_click(&ui, x, y));
    let editing =
        crate::app::with_app_r(&app, |app| app.image.text_edit.is_editing()).ok_or("borrowed")?;
    assert!(editing, "double-click did not open the text editor");
    Ok(())
}

/// Transform mode arms the transform overlay; dragging a handle commits one
/// transform step.
#[test]
fn transform_mode_drag_transforms_the_layer() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| app.image_action(&ui, "edit.transform.distort"));
    let armed =
        crate::app::with_app_r(&app, |app| app.image.transform_mode.is_some()).ok_or("borrowed")?;
    assert!(armed, "transform mode did not arm");

    // Distort's first handle sits on the layer's top-left corner (4,4).
    crate::app::with_app(&app, |app| {
        app.image_tool(&ui, "hand");
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(4.0, 4.0).0,
            at(4.0, 4.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(2.0, 2.0).0,
            at(2.0, 2.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(2.0, 2.0).0,
            at(2.0, 2.0).1,
            Button::Left,
            false,
            false,
        );
    });
    let label = crate::app::with_app_r(&app, |app| last_label(app)).ok_or("borrowed")?;
    assert_eq!(label.as_deref(), Some("Transform"));
    Ok(())
}

/// Hand hit-tests the active layer: a layer that carries pixels is grabbed only
/// where its pixels are opaque; transparent parts never pan the view.
#[test]
fn hand_grabs_the_layer_only_where_its_pixels_are_opaque() -> Result<(), Box<dyn std::error::Error>>
{
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        // One opaque pixel at layer-local (6,6) = document (10,10); the layer
        // sits at (4,4) and is 12x12.
        if let Some(key) = app
            .image
            .project
            .board_layers
            .first()
            .and_then(|layer| layer.storage_key.clone())
        {
            if let Some(bitmap) = app.image.bitmaps.get_mut(&key) {
                bitmap.set(6, 6, [255, 0, 0, 255]);
            }
        }
        app.image_tool(&ui, "hand");
        let before = app.image.view;
        // Layer-local (2,2) is transparent: the press pans instead of dragging.
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(6.0, 6.0).0,
            at(6.0, 6.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(8.0, 6.0).0,
            at(8.0, 6.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(8.0, 6.0).0,
            at(8.0, 6.0).1,
            Button::Left,
            false,
            false,
        );
        assert!(
            (app.image.view.x - before.x).abs() < 0.01,
            "a transparent pixel must not pan the view ({:?})",
            app.image.view
        );
        assert_eq!(
            app.image.project.board_layers[0].x, 4.0,
            "the layer did not move"
        );

        // Reset the view and press the opaque pixel: that drags the layer.
        app.image.view = before;
        app.image_pointer(
            &ui,
            PointerKind::Down,
            at(10.0, 10.0).0,
            at(10.0, 10.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Move,
            at(12.0, 11.0).0,
            at(12.0, 11.0).1,
            Button::Left,
            false,
            false,
        );
        app.image_pointer(
            &ui,
            PointerKind::Up,
            at(12.0, 11.0).0,
            at(12.0, 11.0).1,
            Button::Left,
            false,
            false,
        );
        assert!(
            (app.image.project.board_layers[0].x - 6.0).abs() < 0.01
                && (app.image.project.board_layers[0].y - 5.0).abs() < 0.01,
            "the opaque pixel drags the layer ({} , {})",
            app.image.project.board_layers[0].x,
            app.image.project.board_layers[0].y
        );
        assert_eq!(app.image.view, before, "the layer drag does not pan");
    });
    Ok(())
}

/// Source wiring: the rail's `tool-pick` reaches `App::image_tool`, every rail
/// id round-trips to the ops state machine, and the full-workspace focus
/// scopes never regress to swallowing pointer presses.
#[test]
fn rail_wiring_reaches_the_state_machine() -> Result<(), Box<dyn std::error::Error>> {
    let panels = include_str!("../../ui/components/image-panels.slint");
    let image = include_str!("../../ui/components/image.slint");
    let app = include_str!("../../ui/app.slint");
    let wire = include_str!("wire.rs");
    let render = include_str!("render.rs");
    let ops = include_str!("ops.rs");

    assert!(
        panels.contains("callback tool-pick(string);"),
        "ImageToolRail declares tool-pick"
    );
    assert!(
        panels.contains("for tool in root.tools") && panels.contains("root.tool-pick(tool.id);"),
        "each rail tool button emits tool-pick"
    );
    assert!(
        image.contains("tool-pick(id) =>") && image.contains("root.tool(id);"),
        "ImageStudio forwards tool-pick to its tool callback"
    );
    assert!(
        image.contains("tools: root.tools;"),
        "the rail reads the studio's tool rows"
    );
    assert!(
        app.contains("tools: root.image-tools;"),
        "the studio receives the Rust-built tool rows"
    );
    assert!(
        app.contains("tool(id) =>") && app.contains("root.image-tool(id);"),
        "the app forwards the studio tool callback"
    );
    assert!(
        wire.contains("ui.on_image_tool") && wire.contains("app.image_tool"),
        "the Rust wire registers image-tool and calls App::image_tool"
    );
    assert!(
        render.contains("crate::image::TOOLS") && render.contains("ImageToolRow"),
        "the rail rows are built from TOOLS"
    );

    for (id, _, _) in TOOLS {
        let tool = ImageTool::from_id(id).ok_or_else(|| format!("ops handles {id}"))?;
        assert_eq!(tool.as_str(), id);
        let variant = format!("{tool:?}");
        assert!(
            ops.contains(&format!("ImageTool::{variant}")),
            "the {id} arm (ImageTool::{variant}) is in the state machine"
        );
    }
    assert!(ImageTool::from_id("not-a-tool").is_none());

    // The root causes of the dead-tools bug: a full-workspace FocusScope with
    // the default click-to-focus eats the press that should reach the button.
    let focus_block = image
        .split("image-focus := FocusScope")
        .nth(1)
        .unwrap_or_default();
    assert!(
        focus_block.contains("focus-on-click: false;"),
        "the studio focus scope must not steal the pointer press"
    );
    let app_focus = app
        .split("shortcut-focus := FocusScope")
        .nth(1)
        .unwrap_or_default();
    assert!(
        app_focus.contains("focus-on-click: false;"),
        "the global shortcut scope must not steal the first press of a workspace"
    );
    Ok(())
}

/// The studio's keyboard scope still owns the tool shortcuts: showing the
/// workspace hands it the focus explicitly, so the click-to-focus fix does not
/// leave the shortcuts with the canvas.
#[test]
fn keyboard_shortcuts_reach_the_studio() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    let key = |text: &str| {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: slint::SharedString::from(text),
        });
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: slint::SharedString::from(text),
        });
    };
    key("b");
    let tool = crate::app::with_app_r(&app, |app| app.image.tool.as_str().to_string())
        .ok_or("borrowed")?;
    assert_eq!(tool, "brush", "the B shortcut selects the brush");
    key("e");
    let tool = crate::app::with_app_r(&app, |app| app.image.tool.as_str().to_string())
        .ok_or("borrowed")?;
    assert_eq!(tool, "eraser", "the E shortcut selects the eraser");
    Ok(())
}

/// The whole Slint pointer chain: a real window drag over the board paints the
/// layer, and the rail still takes clicks afterwards. This is the exact
/// gesture that was dead while the studio focus scope ate the press.
#[test]
fn board_drag_paints_through_the_window() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    crate::app::with_app(&app, |app| {
        app.image.tool = ImageTool::Brush;
        app.image.color = "#ff0000".to_string();
        app.image.brush_size = 6.0;
        app.sync_image(&ui);
    });
    // Window coordinates: the 40px studio menu bar places the board at (40, 40);
    // the pinned 4x view maps document (x, y) to board-local (4x, 4y).
    //
    // The brush's floating options card hangs over the canvas's top-left corner,
    // so dismiss it first through its own close affordance (rail 40 + gap 12 +
    // card 236 - padding 10 - button 16 → x 262..278, one padding row under the
    // 40px menu bar).
    click(&ui, 270.0, 58.0);
    let to_window = |doc_x: f32, doc_y: f32| (40.0 + doc_x * 4.0, 40.0 + doc_y * 4.0);
    let (x, y) = to_window(8.0, 8.0);
    let start = slint::LogicalPosition::new(x, y);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: start });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: start,
        button: PointerEventButton::Left,
    });
    for step in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
        let (x, y) = to_window(8.0 + step, 8.0);
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
    }
    let (x, y) = to_window(13.0, 8.0);
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(x, y),
        button: PointerEventButton::Left,
    });
    let painted = crate::app::with_app_r(&app, |app| active_bitmap(app).map(|b| b.get(4, 4)))
        .flatten()
        .ok_or("brush painted no bitmap")?;
    assert!(painted[3] > 0, "the board drag left {painted:?}");

    // And the rail is still live after the drag.
    click(&ui, 20.0, 62.0);
    let selected = crate::app::with_app_r(&app, |app| app.image.tool.as_str().to_string())
        .ok_or("borrowed")?;
    assert_eq!(selected, "hand", "the rail takes clicks after a board drag");
    Ok(())
}

/// Source wiring for the studio chrome: it is a full-window workspace (no app
/// title bar, no app side panel), the menu bar carries the Home destination
/// left of File, rulers start hidden and the dropdowns are compact rows with a
/// drawn check mark.
#[test]
fn studio_chrome_is_full_window_and_its_menus_are_compact() {
    let app = include_str!("../../ui/app.slint");
    let image = include_str!("../../ui/components/image.slint");
    let widgets = include_str!("../../ui/components/studio-widgets.slint");
    let state = include_str!("mod.rs");

    // 1. Full window: there is no app title bar in any view.
    assert!(!app.contains("TopBar"), "the studio has no app title bar");
    // 2. No app side panel: it is declared only in the editor column.
    assert_eq!(
        app.matches("SidePanel {").count(),
        1,
        "the app side panel exists only in the editor view"
    );
    assert!(
        app.contains(r#"visible: root.view == "editor";"#),
        "the side-panel column is editor-only"
    );

    // 3. Home is the first menu-bar control, immediately left of File.
    let home = image
        .find("home-btn := StudioIconButton")
        .unwrap_or(usize::MAX);
    let file = image
        .find("file-btn := StudioMenuTab")
        .unwrap_or(usize::MAX);
    assert!(home < file, "the Home button sits left of File");
    assert!(
        image.contains("icon: root.back-to-canvas ? Icons.back : Icons.home;")
            && image.contains(r#"tip: root.back-to-canvas ? "Back to canvas" : "Home";"#),
        "the first control switches between Home and Back to canvas"
    );
    assert!(
        image.contains("root.home();")
            && app.contains("home => {")
            && app.contains("root.go-home();"),
        "the studio Home emits the app's go-home action"
    );

    // 4. Rulers are an off-by-default View toggle.
    assert!(app.contains("in-out property <bool> image-show-rulers: false;"));
    assert!(image.contains("in-out property <bool> show-rulers: false;"));
    assert!(image.contains("label: \"Rulers\";"));
    assert!(state.contains("show_rulers: false,"));

    // 6. A menu hugs its content, and its rows are fixed 28px rows.
    assert!(
        widgets.contains("height: menu-list.preferred-height;"),
        "the popup height comes from its rows"
    );
    assert!(
        widgets.contains("height: root.row-height;")
            && widgets.contains("root.note == \"\" ? 28px : 38px"),
        "menu rows pin their 28px height"
    );
    assert!(
        widgets.contains("horizontal-alignment: right;"),
        "shortcuts are right-aligned"
    );

    // 7. A drawn 14px check with 2px round caps in the accent colour, with a
    // gutter of the same width on every row.
    assert!(
        widgets.contains("min-width: 14px;") && widgets.contains("width: 14px;"),
        "the check gutter is 14px wide"
    );
    assert!(
        widgets.contains("stroke-width: 3.4px;")
            && widgets.contains("stroke-line-cap: round;")
            && widgets.contains("commands: Icons.check;"),
        "the check is a stroked glyph, not a boxed character"
    );
    assert!(
        widgets.contains(
            "stroke: ta.has-hover ? StudioTheme.brand-foreground : StudioTheme.selection;"
        ),
        "the check uses the selection accent"
    );
}

/// The studio's chrome matches the canvas: no document tab strip, a taller menu
/// bar that the board sits directly under, and the canvas mouse mapping (left
/// drag pans through Hand, right drag moves the window).
#[test]
fn studio_chrome_has_no_tab_strip_and_matches_the_canvas_mouse_mapping() {
    let app = include_str!("../../ui/app.slint");
    let image = include_str!("../../ui/components/image.slint");
    let theme = include_str!("../../ui/studio-theme.slint");

    // 1. No document tab strip: no component, no band, no title state.
    assert!(
        !image.contains("StudioDocTab") && !image.contains("doc-tab-h"),
        "the document tab strip is deleted from the studio"
    );
    assert!(
        !image.contains("in-out property <string> title:"),
        "the studio holds no document title any more"
    );
    assert!(
        !app.contains("image-title"),
        "the app exposes no studio title state"
    );

    // 2. The menu bar is 40px and the board follows it directly.
    assert!(
        theme.contains("out property <length> menu-bar-h: 40px;"),
        "the menu bar token is 40px"
    );
    assert!(
        image.contains("height: StudioTheme.menu-bar-h;")
            && image.contains("size: StudioTheme.menu-bar-h;"),
        "the menu entries and the Home button share the band height"
    );
    assert!(
        image.contains("board-area := Rectangle") && !image.contains("document tab bar"),
        "the board is the menu bar's next sibling"
    );

    // 3. The menu bar is the only window-move surface; the board no longer
    // starts a window drag on the right button.
    assert!(
        !image.contains("board-area.window-dragging"),
        "the board does not move the window"
    );
    assert!(
        image.contains("menu-drag := TouchArea"),
        "the menu bar owns the window move"
    );
    for wiring in [
        "root.drag-begin();",
        "root.drag-tick();",
        "root.drag-end();",
    ] {
        assert!(
            image.contains(wiring),
            "the menu bar wires the window move (`{wiring}`)"
        );
    }
    for wiring in ["drag-begin => {", "drag-tick => {", "drag-end => {"] {
        assert!(
            app.contains(wiring),
            "the app forwards the studio's window move (`{wiring}`)"
        );
    }
}

/// The studio's first control is Home: clicking it runs the app's `go-home`
/// action (the same destination as the library bar's Home button), returning
/// to the canvas library with the side panel state untouched.
#[test]
fn the_menu_bar_home_button_returns_to_the_library() -> Result<(), Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let (ui, app, _dir) = harness()?;
    assert_eq!(ui.get_view(), "image", "the studio is open");

    // The 40px Home button is the menu bar's first control (4px inset, full
    // band height, the same height as the File tab): its box is x 4..44,
    // y 0..40.
    click(&ui, 14.0, 14.0);

    assert_eq!(
        ui.get_view(),
        "library",
        "Home returns to the canvas library"
    );
    assert_eq!(
        app.borrow().view,
        crate::app::View::Library,
        "the app state follows the same path as the app bar's Home"
    );
    Ok(())
}
