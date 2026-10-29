//! Headless render tests of the AI-backed dialogs: the Slint testing backend
//! embeds the software rasterizer, so `Window::take_snapshot` returns a real
//! pixel buffer (project rule: code-only verification). Also covers the
//! video-prompt mode toggle's document write.

use std::cell::RefCell;
use std::rc::Rc;

use slint::Model as _;

use open_canvas_core::node::CanvasNodeType;

use crate::store::Store;
use crate::AppWindow;

use super::*;
use crate::test_support::{differing_pixels, init_backend_allow_existing, render_editor};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn red_surface(size: usize) -> Surface {
    let mut surface = Surface::new(size, size);
    surface.fill([220, 20, 20, 255]);
    surface
}

fn red_pixels(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> usize {
    buffer
        .as_slice()
        .iter()
        .filter(|pixel| pixel.r > 180 && pixel.g < 80 && pixel.b < 80)
        .count()
}

/// A document with one image node carrying a red bitmap; returns its id.
fn image_doc(app: &mut App) -> String {
    app.doc = crate::doc::Doc::empty();
    let now = app.now_ms();
    app.insert_image_node(
        "AI fixture",
        &red_surface(8),
        crate::coords::WorldPos::new(20.0, 20.0),
        now,
    )
    .unwrap_or_default()
}

#[test]
fn ai_image_dialog_paints_its_stage_and_body() -> TestResult {
    init_backend_allow_existing();
    let closed = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            image_doc(app);
            app.sync_all(ui);
        });
    })?;
    let ocr = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app);
            app.sync_all(ui);
            app.image_ai_open(ui, &id, "ocr");
        });
    })?;
    let diff = differing_pixels(&closed, &ocr).map_err(ToString::to_string)?;
    assert!(diff > 50_000, "the OCR dialog covers the shell ({diff})");
    assert!(
        red_pixels(&ocr) > 1_000,
        "the dialog paints the decoded source bitmap"
    );
    let angle = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app);
            app.sync_all(ui);
            app.image_ai_open(ui, &id, "angle");
        });
    })?;
    let diff = differing_pixels(&ocr, &angle).map_err(ToString::to_string)?;
    assert!(
        diff > 5_000,
        "the angle body differs from the OCR body ({diff})"
    );
    let upscale = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app);
            app.sync_all(ui);
            app.image_ai_open(ui, &id, "ai-upscale");
        });
    })?;
    let diff = differing_pixels(&angle, &upscale).map_err(ToString::to_string)?;
    assert!(
        diff > 5_000,
        "the upscale body differs from the angle body ({diff})"
    );
    let segment = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app);
            app.sync_all(ui);
            app.image_ai_open(ui, &id, "segment");
        });
    })?;
    let diff = differing_pixels(&upscale, &segment).map_err(ToString::to_string)?;
    assert!(
        diff > 5_000,
        "the segment body differs from the upscale body ({diff})"
    );
    Ok(())
}

#[test]
fn segment_dialog_reports_the_missing_runtime() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let dir = tempfile::TempDir::new()?;
    let app = Rc::new(RefCell::new(App::new(Store::at(dir.path().to_path_buf()))));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        let id = image_doc(app);
        app.sync_all(&ui);
        app.image_ai_open(&ui, &id, "segment");
        assert!(!ui.get_image_ai_state().runtime_available);
        assert!(
            ui.get_image_ai_state()
                .notice
                .contains("No local segmentation runtime"),
            "the dialog states the missing runtime"
        );
        // Running the request path surfaces the honest unavailable error.
        app.image_ai_apply(&ui);
        let state = ui.get_image_ai_state();
        assert!(state.error, "the unavailable state is an error");
        assert!(
            state.message.contains("No local segmentation runtime"),
            "got {}",
            state.message
        );
    });
    Ok(())
}

#[test]
fn video_mode_toggle_writes_the_prompt_node() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let dir = tempfile::TempDir::new()?;
    let app = Rc::new(RefCell::new(App::new(Store::at(dir.path().to_path_buf()))));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.doc = crate::doc::Doc::empty();
        let now = app.now_ms();
        let id = app.doc.create_node(
            CanvasNodeType::VideoPrompt,
            crate::coords::WorldPos::new(0.0, 0.0),
            now,
        );
        app.sync_all(&ui);
        app.node_video_mode(&ui, &id, "reference");
        let node = app
            .doc
            .node(&id)
            .cloned()
            .unwrap_or_else(|| unreachable!("the fixture node exists"));
        assert_eq!(
            node.metadata.and_then(|meta| meta.video_mode),
            Some(open_canvas_core::node::CanvasVideoMode::Reference)
        );
        // The row carries the wire value the card renders.
        app.sync_rows_of(&ui, std::slice::from_ref(&id));
        let rows = ui.get_nodes();
        let mode = rows
            .iter()
            .find(|row| row.id.as_str() == id.as_str())
            .map(|row| row.video_mode.to_string())
            .unwrap_or_default();
        assert_eq!(mode, "reference");
    });
    Ok(())
}

#[test]
fn video_slot_stepper_moves_within_the_provider_limit() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let dir = tempfile::TempDir::new()?;
    let app = Rc::new(RefCell::new(App::new(Store::at(dir.path().to_path_buf()))));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.doc = crate::doc::Doc::empty();
        let now = app.now_ms();
        let id = app.doc.create_node(
            CanvasNodeType::VideoPrompt,
            crate::coords::WorldPos::new(0.0, 0.0),
            now,
        );
        app.node_video_mode(&ui, &id, "reference");
        app.sync_all(&ui);
        let count_of = |ui: &AppWindow| {
            ui.get_nodes()
                .iter()
                .find(|row| row.id.as_str() == id.as_str())
                .map_or(-1, |row| row.ref_slot_count)
        };
        assert_eq!(count_of(&ui), 3, "the default reference slot count");
        app.node_video_slots(&ui, &id, 1);
        assert_eq!(count_of(&ui), 4, "the stepper adds a slot");
        app.node_video_slots(&ui, &id, -2);
        assert_eq!(count_of(&ui), 2, "the stepper removes slots");
        // Never below one, never above the provider's nine-image limit.
        app.node_video_slots(&ui, &id, -10);
        assert_eq!(count_of(&ui), 1);
        app.node_video_slots(&ui, &id, 100);
        assert_eq!(count_of(&ui), 9);
        // The row's title model always matches the count, one entry per slot.
        let titles = ui
            .get_nodes()
            .iter()
            .find(|row| row.id.as_str() == id.as_str())
            .map_or(0, |row| row.ref_slot_titles.row_count());
        assert_eq!(titles, 9, "one title per reference slot");
    });
    Ok(())
}

#[test]
fn image_ai_open_requires_image_content() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let dir = tempfile::TempDir::new()?;
    let app = Rc::new(RefCell::new(App::new(Store::at(dir.path().to_path_buf()))));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.doc = crate::doc::Doc::empty();
        let now = app.now_ms();
        let id = app.doc.create_node(
            CanvasNodeType::Image,
            crate::coords::WorldPos::new(0.0, 0.0),
            now,
        );
        app.image_ai_open(&ui, &id, "ocr");
        assert!(app.panel.image_ai.is_none(), "an empty node opens nothing");
    });
    Ok(())
}
