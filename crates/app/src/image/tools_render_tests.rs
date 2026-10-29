//! Headless render tests of the canvas media pipeline: the preview modal paints
//! the real bitmap (never the placeholder), the image tool dialog paints its
//! stage, and an empty image node paints only its static hint. Project rule:
//! code-only verification — the Slint testing backend embeds the software
//! rasterizer, so `Window::take_snapshot` returns a real pixel buffer.

use open_canvas_core::node::CanvasNodeType;

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

/// A document with one image node carrying `surface`; returns its id.
fn image_doc(app: &mut App, surface: &Surface) -> Option<String> {
    app.doc = crate::doc::Doc::empty();
    let now = app.now_ms();
    app.insert_image_node("Preview fixture", surface, WorldPos::new(20.0, 20.0), now)
}

#[test]
fn preview_modal_paints_the_real_bitmap() -> TestResult {
    init_backend_allow_existing();
    let baseline = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            image_doc(app, &red_surface(8));
            app.sync_all(ui);
        });
    })?;
    let with_bitmap = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app, &red_surface(8)).unwrap_or_default();
            app.bars.preview_node = Some(id);
            app.sync_all(ui);
            app.sync_modals(ui);
        });
    })?;
    let painted = differing_pixels(&baseline, &with_bitmap).map_err(ToString::to_string)?;
    assert!(
        painted > 50_000,
        "the preview modal covers the shell with the bitmap ({painted})"
    );
    let red = red_pixels(&with_bitmap);
    assert!(
        red > 2_000,
        "the modal paints the decoded bitmap, not the placeholder ({red} red pixels)"
    );
    Ok(())
}

#[test]
fn preview_modal_of_an_empty_node_paints_the_placeholder() -> TestResult {
    init_backend_allow_existing();
    let with_bitmap = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app, &red_surface(8)).unwrap_or_default();
            app.bars.preview_node = Some(id);
            app.sync_all(ui);
            app.sync_modals(ui);
        });
    })?;
    let empty = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            app.doc = crate::doc::Doc::empty();
            let now = app.now_ms();
            let id = app
                .doc
                .create_node(CanvasNodeType::Image, WorldPos::new(20.0, 20.0), now);
            app.bars.preview_node = Some(id);
            app.sync_all(ui);
            app.sync_modals(ui);
        });
    })?;
    assert_eq!(
        red_pixels(&empty),
        0,
        "an empty node never paints the fixture bitmap"
    );
    let diff = differing_pixels(&with_bitmap, &empty).map_err(ToString::to_string)?;
    assert!(
        diff > 20_000,
        "the bitmap and the placeholder differ ({diff})"
    );
    Ok(())
}

#[test]
fn preview_modal_of_a_stack_paints_the_gallery() -> TestResult {
    init_backend_allow_existing();
    let single = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app, &red_surface(8)).unwrap_or_default();
            app.bars.preview_node = Some(id);
            app.sync_all(ui);
            app.sync_modals(ui);
        });
    })?;
    let stack = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            app.doc = crate::doc::Doc::empty();
            let images = [
                ("a".to_string(), red_surface(8)),
                ("b".to_string(), red_surface(6)),
                ("c".to_string(), red_surface(4)),
            ];
            let now = app.now_ms();
            let id =
                app.doc
                    .create_node(CanvasNodeType::ImageStack, WorldPos::new(20.0, 20.0), now);
            if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
                let entries: Vec<_> = images
                    .iter()
                    .enumerate()
                    .map(|(index, (_, surface))| {
                        crate::image::tools::node_image_entry(
                            &format!("stack-{id}-{index}"),
                            surface,
                            None,
                        )
                    })
                    .collect();
                let cover = entries.first().cloned();
                let meta = node
                    .metadata
                    .get_or_insert_with(open_canvas_core::node::CanvasNodeMetadata::default);
                meta.images = Some(entries);
                meta.count = Some(i64::try_from(images.len()).unwrap_or(i64::MAX));
                if let Some(cover) = cover {
                    meta.status = Some(open_canvas_core::node::CanvasNodeStatus::Success);
                    meta.natural_width = Some(cover.natural_width);
                    meta.natural_height = Some(cover.natural_height);
                    meta.bytes = Some(cover.bytes);
                    meta.mime_type = Some(cover.mime_type.clone());
                    meta.content = Some(cover.content.clone());
                    meta.primary_image_id = Some(cover.id.clone());
                }
            }
            app.bars.preview_node = Some(id);
            app.sync_all(ui);
            app.sync_modals(ui);
        });
    })?;
    let diff = differing_pixels(&single, &stack).map_err(ToString::to_string)?;
    assert!(
        diff > 50_000,
        "the stack gallery replaces the single view ({diff})"
    );
    assert!(
        red_pixels(&stack) > 2_000,
        "the gallery paints the decoded thumbs"
    );
    Ok(())
}

#[test]
fn image_tool_dialog_paints_its_stage() -> TestResult {
    init_backend_allow_existing();
    let closed = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            image_doc(app, &red_surface(8));
            app.sync_all(ui);
        });
    })?;
    let crop = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app, &red_surface(8)).unwrap_or_default();
            app.sync_all(ui);
            app.image_tool_open(ui, &id, "crop");
        });
    })?;
    let diff = differing_pixels(&closed, &crop).map_err(ToString::to_string)?;
    assert!(diff > 50_000, "the crop dialog covers the shell ({diff})");
    assert!(
        red_pixels(&crop) > 1_000,
        "the crop stage paints the decoded bitmap"
    );
    let analysis = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            let id = image_doc(app, &red_surface(8)).unwrap_or_default();
            app.sync_all(ui);
            app.image_tool_open(ui, &id, "analysis");
        });
    })?;
    let diff = differing_pixels(&crop, &analysis).map_err(ToString::to_string)?;
    assert!(
        diff > 5_000,
        "the analysis body differs from the crop body ({diff})"
    );
    Ok(())
}

#[test]
fn an_empty_image_node_paints_only_its_static_hint() -> TestResult {
    init_backend_allow_existing();
    let closed = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            app.sync_all(ui);
        });
    })?;
    let empty = render_editor(|ui, app| {
        crate::app::with_app(app, |app| {
            app.doc.create_node(
                CanvasNodeType::Image,
                WorldPos::new(120.0, 120.0),
                app.now_ms(),
            );
            app.sync_all(ui);
        });
    })?;
    let diff = differing_pixels(&closed, &empty).map_err(ToString::to_string)?;
    assert!(
        diff > 2_000,
        "the empty image card paints its hint ({diff})"
    );
    // No media bitmap: the only red-ish pixels are a handful of antialiased
    // accent edges, never a decoded image.
    assert!(
        red_pixels(&empty) < 1_000,
        "an empty image node paints no media ({} red pixels)",
        red_pixels(&empty)
    );
    Ok(())
}
