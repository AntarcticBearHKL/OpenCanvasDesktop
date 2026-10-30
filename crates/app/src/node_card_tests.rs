//! Node-card layout tests (project rule: code-only verification -- no GUI, no
//! screenshots).
//!
//! 1. One rendered probe per built-in card kind, at its default size from
//!    `open_canvas_core::constants`: the heavy card is compared with a bare
//!    card of the same size, so any pixel the body paints past the card's
//!    bottom (or right) edge shows up as a difference. The same diff proves the
//!    body is not empty.
//! 2. Source guards for the two layout defects: a video-prompt card owns
//!    exactly one prompt editor and one frame-slot row, and the card body takes
//!    its padding/spacing from the `Theme.space-*` scale.

use slint::ComponentHandle;
use slint::Model as _;

use crate::test_support::init_backend_allow_existing;
use open_canvas_core::constants::get_node_spec;
use open_canvas_core::node::CanvasNodeType;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The probe renders the real `NodeCard` on a flat green field, so every pixel
/// outside the card's own rect that is not green came from the card body.
mod probe {
    slint::slint! {
        import { NodeCard } from "../ui/components/node-card.slint";
        import {
            AssetItem, RefImage, StackImage,
        } from "../ui/components/overlay-types.slint";

        export component NodeCardProbe inherits Window {
            in property <string> kind: "";
            in property <length> card-w: 340px;
            in property <length> card-h: 240px;
            // The canvas zoom the card is rendered at: the card rect and every
            // internal length scale together.
            in property <float> probe-scale: 1;
            in property <length> window-w: 520px;
            in property <length> window-h: 760px;
            // A `bare` card keeps the same size and title but has no body, so
            // it paints the card chrome only.
            in property <bool> bare: false;
            in property <bool> can-generate: false;
            in property <string> mode: "image";
            in property <string> body: "";
            // Video-prompt probe: the mode tab and the editable reference count.
            in property <string> probe-video-mode: "frames";
            in property <int> probe-ref-count: 3;
            property <[string]> probe-ref-titles: [
                "ref A", "ref B", "ref C", "ref D", "ref E",
                "ref F", "ref G", "ref H", "ref I",
            ];
            in property <int> stack-count: 0;
            in property <string> probe-status: "";
            in property <string> probe-error: "";
            in property <bool> probe-prompt-editing: false;
            in property <string> probe-asset-count-label: "10 items";
            in property <int> probe-attempt-version: 1;
            in property <string> probe-attempt-title: "";
            in property <string> probe-attempt-reason: "";
            in property <image> content-image;
            property <image> blank-image;

            background: #00ff00;
            width: root.window-w;
            height: root.window-h;

            property <[RefImage]> probe-refs: [
                { id: "ref-1", title: "ref 1", thumb: root.blank-image },
                { id: "ref-2", title: "ref 2", thumb: root.blank-image },
                { id: "ref-3", title: "ref 3", thumb: root.blank-image },
                { id: "ref-4", title: "ref 4", thumb: root.blank-image },
                { id: "ref-5", title: "ref 5", thumb: root.blank-image },
                { id: "ref-6", title: "ref 6", thumb: root.blank-image },
                { id: "ref-7", title: "ref 7", thumb: root.blank-image },
                { id: "ref-8", title: "ref 8", thumb: root.blank-image },
                { id: "ref-9", title: "ref 9", thumb: root.blank-image },
                { id: "ref-10", title: "ref 10", thumb: root.blank-image },
            ];
            property <[AssetItem]> probe-assets: [
                { id: "asset-1", label: "Output 1", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-2", label: "Output 2", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-3", label: "Output 3", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-4", label: "Output 4", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-5", label: "Output 5", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-6", label: "Output 6", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-7", label: "Output 7", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-8", label: "Output 8", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-9", label: "Output 9", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
                { id: "asset-10", label: "Output 10", sub: "PNG", kind: "image", thumb: root.blank-image, ready: true },
            ];
            property <[StackImage]> probe-stack: [
                { id: "stack-1", thumb: root.blank-image, dx: 3px, dy: 5px, angle: 1.5 },
                { id: "stack-2", thumb: root.blank-image, dx: -4px, dy: 9px, angle: -2.1 },
                { id: "stack-3", thumb: root.blank-image, dx: 6px, dy: 13px, angle: 2.8 },
                { id: "stack-4", thumb: root.blank-image, dx: -8px, dy: 17px, angle: -3.4 },
            ];

            NodeCard {
                canvas-x: 40px;
                canvas-y: 80px;
                card-width: root.card-w;
                card-height: root.card-h;
                scale: root.probe-scale;
                kind: root.bare ? "probe-bare" : root.kind;
                title: "Probe";
                body: root.body;
                status: root.probe-status;
                error-text: root.probe-error;
                prompt-editing: root.probe-prompt-editing;
                content-image: root.content-image;
                content-empty: root.content-image.width == 0;
                stack: root.probe-stack;
                stack-count: root.stack-count;
                stack-count-label: "4 images";
                assets: root.probe-assets;
                asset-count: root.probe-assets.length;
                asset-count-label: root.probe-asset-count-label;
                asset-source: "studio";
                duration: "00:12";
                status-label: "Ready";
                ref-images: root.probe-refs;
                model: "model-one";
                model-brand: "M";
                mode: root.mode;
                can-generate: root.can-generate;
                video-mode: root.probe-video-mode;
                slot-first: "First frame";
                slot-last: "Last frame";
                ref-slot-count: root.probe-ref-count;
                ref-slot-titles: root.probe-ref-titles;
                attempt-version: root.probe-attempt-version;
                attempt-title: root.probe-attempt-title;
                attempt-reason: root.probe-attempt-reason;
            }
        }
    }
}

/// The probe's layout in logical pixels; the software rasterizer runs at 1:1.
const CARD_X: u32 = 40;
const CARD_Y: u32 = 80;
const WINDOW_W: u32 = 520;
const WINDOW_H: u32 = 760;

/// A solid 8x8 bitmap, so a media card paints real pixels in its body.
fn red_image() -> slint::Image {
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(8, 8);
    for pixel in buffer.make_mut_slice() {
        *pixel = slint::Rgba8Pixel {
            r: 220,
            g: 20,
            b: 20,
            a: 255,
        };
    }
    slint::Image::from_rgba8(buffer)
}

fn render(
    node_type: &CanvasNodeType,
    kind: &str,
    bare: bool,
    setup: impl FnOnce(&probe::NodeCardProbe),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    render_at(node_type, kind, 1.0, bare, setup)
}

/// Renders the card as the canvas shows it at `scale`: the card rect and the
/// internal `visual-scale` both grow by the same factor.
fn render_at(
    node_type: &CanvasNodeType,
    kind: &str,
    scale: f64,
    bare: bool,
    setup: impl FnOnce(&probe::NodeCardProbe),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    let spec = get_node_spec(node_type);
    let ui = probe::NodeCardProbe::new()?;
    ui.set_kind(kind.into());
    ui.set_card_w(default_size(spec.width * scale));
    ui.set_card_h(default_size(spec.height * scale));
    ui.set_probe_scale(float(scale));
    ui.set_window_w(default_size(spec.width * scale) + float(f64::from(CARD_X)) * 2.0);
    ui.set_window_h(default_size(spec.height * scale) + float(f64::from(CARD_Y)) + 40.0);
    ui.set_bare(bare);
    if bare {
        ui.set_can_generate(false);
    } else {
        setup(&ui);
    }
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// A small, exact factor for the probe's `float` properties.
#[expect(
    clippy::cast_possible_truncation,
    reason = "test scales are 0.5..3 and font sizes stay well inside f32's exact range"
)]
fn float(value: f64) -> f32 {
    value as f32
}

/// A non-negative whole device-pixel value derived from the test's design
/// lengths. The four probe scales land on whole pixels for every length used
/// here, so this is exact rather than merely truncating.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test scales 0.5..3 keep every design length non-negative and below 2^32"
)]
fn px(value: f64) -> u32 {
    value as u32
}

/// The spec sizes are whole logical pixels (340, 240, ...).
#[expect(
    clippy::cast_possible_truncation,
    reason = "node default sizes are whole logical pixels"
)]
fn default_size(value: f64) -> f32 {
    value as f32
}

/// The spec sizes are whole logical pixels, so the card's device-pixel rect is
/// exact at 1:1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "node default sizes are whole, non-negative logical pixels"
)]
fn default_pixels(value: f64) -> u32 {
    value as u32
}

/// Pixels that differ between the two renders inside `[x0, x1) x [y0, y1)`.
fn diff_rect(
    actual: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    bare: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) -> usize {
    let width = actual.width().min(bare.width());
    let height = actual.height().min(bare.height());
    let mut diff = 0;
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let index = (y * width + x) as usize;
            let a = actual.as_slice()[index];
            let b = bare.as_slice()[index];
            if a.r != b.r || a.g != b.g || a.b != b.b || a.a != b.a {
                diff += 1;
            }
        }
    }
    diff
}

/// Renders `kind` at its default size with a deliberately heavy payload and
/// asserts the body paints inside the card only.
fn assert_card_fits(
    label: &str,
    node_type: &CanvasNodeType,
    kind: &str,
    setup: impl FnOnce(&probe::NodeCardProbe),
) -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(node_type);
    let right = CARD_X + default_pixels(spec.width);
    let bottom = CARD_Y + default_pixels(spec.height);
    let actual = render(node_type, kind, false, setup)?;
    let bare = render(node_type, kind, true, |_| {})?;

    let below = diff_rect(&actual, &bare, 0, bottom, WINDOW_W, WINDOW_H);
    assert_eq!(
        below, 0,
        "{label}: {below} body pixels paint past the card's bottom edge ({bottom}px)"
    );
    let beside = diff_rect(&actual, &bare, right, CARD_Y, WINDOW_W, WINDOW_H);
    assert_eq!(
        beside, 0,
        "{label}: {beside} body pixels paint past the card's right edge ({right}px)"
    );
    let inside = diff_rect(
        &actual,
        &bare,
        CARD_X + 2,
        CARD_Y + 2,
        right - 2,
        bottom - 2,
    );
    assert!(
        inside > 500,
        "{label}: the body is not empty ({inside} content pixels)"
    );
    Ok(())
}

const LONG_TEXT: &str = "A deliberately long body that would overflow a card \
    without the scrolling body, wrapping across many lines so the layout has \
    to clip at the card edge instead of painting outside it.";

#[test]
fn image_card_fits_its_default_size() -> TestResult {
    assert_card_fits("image", &CanvasNodeType::Image, "image", |ui| {
        ui.set_content_image(red_image());
    })
}

#[test]
fn video_card_fits_its_default_size() -> TestResult {
    assert_card_fits("video", &CanvasNodeType::Video, "video", |ui| {
        ui.set_content_image(red_image());
    })
}

#[test]
fn image_stack_card_fits_its_default_size() -> TestResult {
    assert_card_fits(
        "image-stack",
        &CanvasNodeType::ImageStack,
        "image-stack",
        |ui| {
            ui.set_stack_count(4);
        },
    )
}

#[test]
fn assets_card_fits_its_default_size() -> TestResult {
    assert_card_fits("assets", &CanvasNodeType::Assets, "assets", |_| {})
}

/// The assets card has no footer strip: the count label is not painted at all,
/// and the strip above the bottom padding stays card chrome (identical to a
/// bare card). Source guard: nothing reads `asset-count-label` in markup.
#[test]
fn assets_card_has_no_footer_row() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Assets);
    let width = default_pixels(spec.width);
    let bottom = CARD_Y + default_pixels(spec.height);
    let with_count = render(&CanvasNodeType::Assets, "assets", false, |ui| {
        ui.set_probe_asset_count_label("10 items".into());
    })?;
    let other_count = render(&CanvasNodeType::Assets, "assets", false, |ui| {
        ui.set_probe_asset_count_label("9999 items".into());
    })?;
    assert_eq!(
        diff_rect(&with_count, &other_count, 0, 0, WINDOW_W, WINDOW_H),
        0,
        "the count label is gone, not merely restyled"
    );

    let bare = render(&CanvasNodeType::Assets, "assets", true, |_| {})?;
    let strip = diff_rect(
        &with_count,
        &bare,
        CARD_X + 2,
        bottom.saturating_sub(16),
        CARD_X + width - 2,
        bottom.saturating_sub(2),
    );
    assert_eq!(
        strip, 0,
        "the bottom strip paints only card chrome ({strip} body pixels)"
    );

    let source = include_str!("../ui/components/node-card.slint");
    assert!(
        !source.contains("root.asset-count-label"),
        "the assets footer no longer reads the count label"
    );
    Ok(())
}

#[test]
fn text_card_fits_its_default_size() -> TestResult {
    assert_card_fits("text", &CanvasNodeType::Text, "text", |ui| {
        ui.set_body(LONG_TEXT.into());
    })
}

#[test]
fn text_generation_card_fits_its_default_size() -> TestResult {
    assert_card_fits("text generation", &CanvasNodeType::Text, "text", |ui| {
        ui.set_body(LONG_TEXT.into());
        ui.set_can_generate(true);
        ui.set_mode("text".into());
    })
}

#[test]
fn prompt_card_fits_its_default_size() -> TestResult {
    assert_card_fits("prompt", &CanvasNodeType::Prompt, "prompt", |ui| {
        ui.set_body(LONG_TEXT.into());
    })
}

#[test]
fn video_prompt_card_fits_its_default_size() -> TestResult {
    assert_card_fits(
        "video-prompt",
        &CanvasNodeType::VideoPrompt,
        "video-prompt",
        |ui| {
            ui.set_body(LONG_TEXT.into());
        },
    )
}

/// The slot row follows the mode tab and the stepper: `frames` renders the two
/// keyframe slots, `reference` renders N reference slots, and changing N
/// repaints the row.
#[test]
fn video_prompt_slots_follow_the_mode_and_the_count() -> TestResult {
    init_backend_allow_existing();
    let frames = render(&CanvasNodeType::VideoPrompt, "video-prompt", false, |ui| {
        ui.set_probe_video_mode("frames".into());
        ui.set_body("a prompt".into());
    })?;
    let two_refs = render(&CanvasNodeType::VideoPrompt, "video-prompt", false, |ui| {
        ui.set_probe_video_mode("reference".into());
        ui.set_probe_ref_count(2);
        ui.set_body("a prompt".into());
    })?;
    let five_refs = render(&CanvasNodeType::VideoPrompt, "video-prompt", false, |ui| {
        ui.set_probe_video_mode("reference".into());
        ui.set_probe_ref_count(5);
        ui.set_body("a prompt".into());
    })?;
    // The card sits at (40, 80) with the spec's default size; compare the whole
    // window, the slot row is the only difference between the three renders.
    let frames_vs_refs = diff_rect(&frames, &two_refs, 0, 0, WINDOW_W, WINDOW_H);
    assert!(
        frames_vs_refs > 300,
        "the reference tab replaces the keyframe row ({frames_vs_refs} px)"
    );
    let two_vs_five = diff_rect(&two_refs, &five_refs, 0, 0, WINDOW_W, WINDOW_H);
    assert!(
        two_vs_five > 20,
        "the stepper changes the slot row ({two_vs_five} px)"
    );
    Ok(())
}

#[test]
fn image_generation_card_fits_its_default_size() -> TestResult {
    assert_card_fits(
        "image-generation",
        &CanvasNodeType::ImageGeneration,
        "image-generation",
        |ui| {
            ui.set_can_generate(true);
            ui.set_mode("image".into());
        },
    )
}

#[test]
fn video_generation_card_fits_its_default_size() -> TestResult {
    assert_card_fits(
        "video-generation",
        &CanvasNodeType::VideoGeneration,
        "video-generation",
        |ui| {
            ui.set_can_generate(true);
            ui.set_mode("video".into());
        },
    )
}

// ---------------------------------------------------------------------------
// Prompt interaction: single click drags, double click edits
// ---------------------------------------------------------------------------

/// The prompt body's resting state is read-only (a single click falls through
/// to the card's drag gesture and never opens the editor). A double click is
/// what enters edit mode, and Escape restores the pre-edit text.
#[test]
fn prompt_single_click_drags_while_double_click_edits() -> TestResult {
    use crate::coords::ScreenPos;
    use crate::input::{Button, PointerEvent, PointerKind};

    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;

    ui.invoke_create_node("prompt".into());
    let id = crate::app::with_app_r(&app, |app| {
        app.doc.nodes().first().map(|node| node.id.clone())
    })
    .flatten()
    .ok_or("the dock creates a prompt node")?;

    let press = |kind| PointerEvent {
        kind,
        pos: ScreenPos::new(40.0, 60.0),
        button: Button::Left,
        shift: false,
        ctrl: false,
    };
    crate::app::with_app(&app, |app| {
        app.node_pointer(&ui, &id, &press(PointerKind::Down));
        assert!(
            app.panel.prompt_editing.is_none(),
            "a single press is the drag gesture, not the editor"
        );
        app.node_pointer(&ui, &id, &press(PointerKind::Up));
        assert!(
            app.panel.prompt_editing.is_none(),
            "a single click never opens the editor"
        );

        app.node_double_click(&ui, &id);
        assert_eq!(
            app.panel.prompt_editing.as_deref(),
            Some(id.as_str()),
            "a double click opens the editor"
        );

        // Type-through goes to the document; Escape restores the origin.
        app.panel_prompt_edited(&ui, "changed", 7);
        assert_eq!(
            app.doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.as_deref()),
            Some("changed")
        );
        app.panel_prompt_edit_cancel(&ui);
        assert!(
            app.panel.prompt_editing.is_none(),
            "Escape leaves the editor"
        );
        assert_eq!(
            app.doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.as_deref()),
            Some(""),
            "Escape restores the pre-edit text"
        );

        // Re-open and click outside (the canvas background): commit and leave.
        app.node_double_click(&ui, &id);
        assert_eq!(app.panel.prompt_editing.as_deref(), Some(id.as_str()));
        app.panel_prompt_edited(&ui, "committed", 9);
        app.canvas_pointer(&ui, &press(PointerKind::Down));
        assert!(
            app.panel.prompt_editing.is_none(),
            "a click outside commits the editor"
        );
        assert_eq!(
            app.doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.as_deref()),
            Some("committed"),
            "the click-outside commit keeps the typed text"
        );
    });
    Ok(())
}

/// Losing focus commits and closes the editor through the existing blur path.
#[test]
fn prompt_editor_commits_on_blur() -> TestResult {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.invoke_create_node("prompt".into());
    let id = crate::app::with_app_r(&app, |app| {
        app.doc.nodes().first().map(|node| node.id.clone())
    })
    .flatten()
    .ok_or("the dock creates a prompt node")?;

    crate::app::with_app(&app, |app| {
        app.panel_prompt_edit_begin(&ui, &id);
        app.panel_prompt_edited(&ui, "kept", 4);
        app.panel_prompt_blur(&ui);
        assert!(app.panel.prompt_editing.is_none(), "blur closes the editor");
    });
    assert_eq!(
        crate::app::with_app_r(&app, |app| app
            .doc
            .node(&id)
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.prompt.clone()))
        .flatten()
        .as_deref(),
        Some("kept"),
        "blur commits the typed text"
    );
    Ok(())
}

/// The resting prompt body has no `TextInput`, the editor is gated by the
/// double-click state, and the empty reference hint line is gone.
#[test]
fn the_prompt_body_rests_read_only_and_has_no_reference_hint() -> TestResult {
    let card = card_source();
    assert!(
        !card.contains("Connect image nodes as references"),
        "the empty reference hint line is gone"
    );
    let input = card
        .find("prompt-input := TextInput {")
        .ok_or("the prompt edit overlay exists")?;
    let overlay = card.get(input..input + 260).unwrap_or("");
    assert!(
        overlay.contains("visible: root.prompt-editing;"),
        "the TextInput only exists while editing"
    );
    assert!(
        card.contains("visible: !root.prompt-editing;"),
        "the resting body is the read-only Text"
    );
    assert_eq!(
        card.matches("text <=> root.prompt-buffer").count(),
        1,
        "exactly one editor, and it is the overlay"
    );
    Ok(())
}

/// The resting prompt body has no line cap: it wraps into every pixel the card
/// gives it and clips only at the card's bottom edge. A short prompt shows in
/// full; a long one is hidden only when the height is genuinely full.
#[test]
fn the_resting_prompt_body_wraps_into_the_whole_card_without_a_line_cap() -> TestResult {
    let card = card_source();
    let start = card
        .find("prompt-text := Rectangle {")
        .ok_or("the prompt body exists")?;
    let end = card
        .find("prompt-input := TextInput {")
        .ok_or("the editor follows the resting body")?;
    let body = card.get(start..end).ok_or("the resting body block")?;
    assert!(
        body.contains("wrap: word-wrap;"),
        "the prompt body wraps instead of truncating a line"
    );
    assert!(
        body.contains("overflow: clip;"),
        "the body clips at the card edge, not with an ellipsis"
    );
    assert!(
        !body.contains("max-lines:"),
        "the body must not cap the number of visible lines"
    );
    assert!(
        !body.contains("overflow: elide;"),
        "the body must not replace hidden lines with an ellipsis"
    );
    assert!(
        body.contains("height: 100%;"),
        "the body fills the card height before clipping"
    );
    Ok(())
}

/// Generation has no Stop button: the card disables while the task runs, and
/// deleting the loading image node is the stop gesture.
#[test]
fn generation_has_no_stop_button_and_delete_is_the_stop_gesture() {
    let card = card_source();
    assert!(!card.contains("\"Stop\""), "no generation card says Stop");
    assert!(
        !card.contains("stop-square"),
        "no generation card paints the stop-square glyph"
    );
    let node_ui = include_str!("node_ui.rs");
    let generate = node_ui
        .split("pub fn node_generate")
        .nth(1)
        .and_then(|rest| rest.split("/// Removes the wire").next())
        .expect("the node_generate function");
    assert!(generate.contains("ai_start"), "Generate starts a task");
    assert!(
        !generate.contains("ai_cancel"),
        "Generate never toggles into a cancel"
    );
    for source in [include_str!("shortcuts.rs"), include_str!("bar_events.rs")] {
        assert!(
            source.contains("cancel_generation_for_node"),
            "every delete path cancels a loading placeholder first"
        );
    }
}

fn card_source() -> String {
    include_str!("../ui/components/node-card.slint").replace("\r\n", "\n")
}

/// A video-prompt card has ONE prompt branch: the editor, the chip/strip rows
/// and (only for `video-prompt`) the mode chips + the mode-dependent slot rows
/// as its tail. The frames row is the two keyframe slots; the reference row is
/// one template slot repeated N times, which is why three `FrameSlot` sites
/// remain in the source.
#[test]
fn video_prompt_renders_one_prompt_editor_and_one_slot_row() -> TestResult {
    let card = card_source();
    let merged =
        "if root.status == \"\" && (root.kind == \"prompt\" || root.kind == \"video-prompt\")";
    assert_eq!(
        card.matches(merged).count(),
        1,
        "the prompt body is one branch for both prompt kinds"
    );
    assert_eq!(
        card.matches("text <=> root.prompt-buffer").count(),
        1,
        "exactly one prompt editor"
    );
    assert_eq!(
        card.matches("FrameSlot {").count(),
        3,
        "two keyframe slots plus the reference-slot template"
    );
    assert_eq!(
        card.matches("VideoModeChip {").count(),
        2,
        "exactly one Frames/Reference chip pair"
    );

    let branch = card.find(merged).ok_or("the merged prompt branch exists")?;
    let editor = card
        .find("text <=> root.prompt-buffer")
        .ok_or("the prompt editor exists")?;
    let tail = card
        .find("if root.kind == \"video-prompt\" : VerticalLayout {")
        .ok_or("the video-prompt tail exists")?;
    assert!(
        branch < editor && editor < tail,
        "the frame slots live inside the one prompt branch, after its editor"
    );
    assert_eq!(
        card.matches("if root.kind == \"video-prompt\" : VerticalLayout {")
            .count(),
        1,
        "no duplicated video-prompt branch"
    );
    Ok(())
}

/// The card body's rhythm is the shared `Theme.space-*` scale, and the title
/// slot is inset like the body and scaled by `visual-scale` only.
#[test]
fn card_body_and_title_share_the_space_rhythm() -> TestResult {
    let card = card_source();
    assert!(
        card.contains("padding: root.frameless ? 0px : Theme.space-4 * root.visual-scale;"),
        "the body pads with space-4 (none for a frameless node)"
    );
    assert!(
        card.contains("spacing: Theme.space-3 * root.visual-scale;"),
        "the body spaces rows with space-3"
    );
    assert!(!card.contains("padding: 16px"), "no bare 16px body padding");
    assert!(!card.contains("spacing: 10px"), "no bare 10px body spacing");

    for line in card.lines() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }
        assert!(!line.contains("-28px"), "`{line}` must use the space scale");
        for key in ["padding:", "padding-left:", "padding-right:", "spacing:"] {
            if let Some(rest) = line.strip_prefix(key) {
                let value = rest.trim();
                assert!(
                    value.starts_with("Theme.space-")
                        || value.starts_with("0px")
                        || (value.starts_with("root.frameless ? 0px : Theme.space-")),
                    "`{line}` must use a Theme.space-* token"
                );
            }
        }
    }

    let title_start = card
        .find("// Title slot (above the card)")
        .ok_or("the title slot exists")?;
    let body_start = card
        .find("// The card body: one rhythm")
        .ok_or("the body exists")?;
    let title = &card[title_start..body_start];
    assert!(
        !title.contains("* root.scale"),
        "the title slot must scale with visual-scale, never root.scale"
    );
    assert!(
        title.contains("x: Theme.space-4 * root.visual-scale;"),
        "the title shares the body's space-4 inset"
    );
    assert!(
        title.contains("y: -(Theme.space-6 * root.visual-scale);"),
        "the title sits at -space-6"
    );
    assert!(
        title.contains("height: 20px * root.visual-scale;"),
        "the title slot is 20px tall"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Generation card design: label column, full-width model row, pinned button
// ---------------------------------------------------------------------------

fn pixel_sum(pixel: slint::Rgba8Pixel) -> u32 {
    u32::from(pixel.r) + u32::from(pixel.g) + u32::from(pixel.b)
}

/// Text/ink: anything clearly darker than the card's near-white body.
fn is_ink(pixel: slint::Rgba8Pixel) -> bool {
    pixel_sum(pixel) < 600
}

/// The brand fill of the Generate button (#D85C3A in the light theme).
fn is_brand(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r > 180 && pixel.g < 130 && pixel.b < 110
}

/// The soft inset plate of the output well (`Theme.muted` = #F1ECE4).
fn is_plate(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r.abs_diff(241) <= 2 && pixel.g.abs_diff(236) <= 2 && pixel.b.abs_diff(228) <= 2
}

/// Bounding box `(x0, y0, x1, y1)` of the pixels matching `predicate` inside
/// the given rect. `y1`/`x1` are exclusive.
fn bbox_where(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    predicate: impl Fn(slint::Rgba8Pixel) -> bool,
) -> Option<(u32, u32, u32, u32)> {
    let width = buffer.width();
    let height = buffer.height();
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let pixel = buffer.as_slice()[(y * width + x) as usize];
            if predicate(pixel) {
                found = Some(match found {
                    None => (x, y, x + 1, y + 1),
                    Some((ux0, uy0, ux1, uy1)) => {
                        (ux0.min(x), uy0.min(y), ux1.max(x + 1), uy1.max(y + 1))
                    }
                });
            }
        }
    }
    found
}

#[expect(
    clippy::too_many_lines,
    reason = "one layout contract: parameters, no well, the pinned button and its centred icon row"
)]
fn assert_generation_card_layout(
    label: &str,
    node_type: &CanvasNodeType,
    kind: &str,
    mode: &str,
) -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(node_type);
    let buffer = render(node_type, kind, false, |ui| {
        ui.set_can_generate(true);
        ui.set_mode(mode.into());
    })?;

    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let inner_left = left + 16;
    let inner_right = right - 16;
    let params_top = top + 16;

    // The label column is left-aligned at the body's 16px inset, and it really
    // paints (an 11px muted label in the fixed 40px slot).
    let label_ink = bbox_where(
        &buffer,
        inner_left,
        params_top,
        inner_left + 40,
        bottom,
        is_ink,
    )
    .ok_or_else(|| format!("{label}: the label column paints"))?;
    assert!(
        label_ink.0 <= inner_left + 3,
        "{label}: labels start at the body's left inset ({} vs {inner_left})",
        label_ink.0
    );
    assert!(
        label_ink.2 > inner_left + 8,
        "{label}: label glyphs have width"
    );

    // The model row is a static full-width label: the fixed model name paints,
    // and the dropdown chevron / picker list are gone.
    let model_ink = bbox_where(
        &buffer,
        inner_left + 44,
        params_top,
        inner_right,
        params_top + 30,
        is_ink,
    );
    assert!(
        model_ink.is_some(),
        "{label}: the fixed model name paints in the model row"
    );
    let panel = include_str!("../ui/components/panel-popovers.slint");
    assert!(
        !panel.contains("chevron-down") && !panel.contains("pick-model"),
        "{label}: the model row is no longer a picker"
    );

    // No output well: the old preview plate is gone (results land in a
    // downstream node), so the middle of the body is plain card fill. A plate
    // band would still paint mostly `node-fill`-like rows here.
    let inner_width = inner_right - inner_left;
    let plate_rows: Vec<u32> = (params_top..bottom)
        .filter(|y| {
            (inner_left..inner_right)
                .filter(|x| is_plate(buffer.as_slice()[(y * buffer.width() + x) as usize]))
                .count()
                * 2
                >= inner_width as usize * 3 / 4
        })
        .collect();
    assert!(
        plate_rows.is_empty(),
        "{label}: no output well paints ({:?} plate rows)",
        plate_rows.first()
    );
    let card = include_str!("../ui/components/node-card.slint");
    assert!(
        !card.contains("No output yet"),
        "{label}: the placeholder text is gone"
    );

    // The Generate button is pinned to the bottom: a solid brand band, full
    // width, 36px tall, ending on the body's 16px inset.
    let button = bbox_where(
        &buffer,
        inner_left,
        bottom - 56,
        inner_right,
        bottom,
        is_brand,
    )
    .ok_or_else(|| format!("{label}: the Generate button paints"))?;
    let button_height = button.3 - button.1;
    let button_width = button.2 - button.0;
    assert!(
        (30..=40).contains(&button_height),
        "{label}: the button is 36px tall ({button_height})"
    );
    assert!(
        button_width + 6 >= inner_right - inner_left,
        "{label}: the button is full width ({button_width})"
    );
    assert!(
        button.3 + 2 >= bottom - 16 && button.3 <= bottom - 16 + 2,
        "{label}: the button ends on the bottom inset ({} vs {})",
        button.3,
        bottom - 16
    );
    let below = bbox_where(&buffer, inner_left, button.3, inner_right, bottom, is_brand);
    assert!(
        below.is_none(),
        "{label}: nothing brand-coloured sits under the button"
    );

    // The button's icon and label share one centred row; the play Path gets an
    // explicit vertical centring (its own viewbox sits high).
    let button_src = card
        .split("generate-btn := Rectangle {")
        .nth(1)
        .and_then(|rest| rest.split("// Plugin node content").next())
        .unwrap_or("");
    assert!(
        button_src.contains("cross-axis-alignment: center;"),
        "{label}: the button row is vertically centred"
    );
    assert!(
        button_src.contains("y: (parent.height - self.height) / 2;"),
        "{label}: the play glyph has an explicit centred y"
    );

    // Measured: the icon and the label share one centre line. Split the light
    // foreground ink inside the button into its two x-clusters (icon, label)
    // and compare their vertical centres.
    let light = |pixel: slint::Rgba8Pixel| pixel.r > 230 && pixel.g > 230 && pixel.b > 230;
    let mut columns: Vec<bool> = Vec::new();
    for x in button.0..button.2.min(buffer.width()) {
        let painted = (button.1..button.3.min(buffer.height()))
            .any(|y| light(buffer.as_slice()[(y * buffer.width() + x) as usize]));
        columns.push(painted);
    }
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut start: Option<u32> = None;
    for (index, painted) in columns.iter().enumerate() {
        let x = button.0 + u32::try_from(index).unwrap_or(0);
        match (painted, start) {
            (true, None) => start = Some(x),
            (false, Some(first)) => {
                runs.push((first, x.saturating_sub(1)));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        runs.push((first, button.2.saturating_sub(1)));
    }
    let runs: Vec<(u32, u32)> = runs.into_iter().filter(|(a, b)| b > a).collect();
    assert!(
        runs.len() >= 2,
        "{label}: the button paints an icon and a label ({runs:?})"
    );
    let vertical_centre = |(x0, x1): (u32, u32)| -> Option<u32> {
        let ink = bbox_where(&buffer, x0, button.1, x1 + 1, button.3, light)?;
        Some(u32::midpoint(ink.1, ink.3))
    };
    if let (Some(icon), Some(text)) = (
        vertical_centre(runs[0]),
        vertical_centre(*runs.last().unwrap_or(&runs[1])),
    ) {
        assert!(
            icon.abs_diff(text) <= 2,
            "{label}: icon centre {icon} and label centre {text} share the row centre"
        );
    }
    Ok(())
}

#[test]
fn image_generation_card_has_the_redesigned_body() -> TestResult {
    assert_generation_card_layout(
        "image-generation",
        &CanvasNodeType::ImageGeneration,
        "image-generation",
        "image",
    )
}

#[test]
fn video_generation_card_has_the_redesigned_body() -> TestResult {
    assert_generation_card_layout(
        "video-generation",
        &CanvasNodeType::VideoGeneration,
        "video-generation",
        "video",
    )
}

/// Image nodes are pure pictures at rest: the card's own edge paints no border
/// and no fill, so the test probe's green background shows through where a
/// normal card would paint chrome.
#[test]
fn image_node_edges_paint_no_border() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Image);
    let buffer = render(&CanvasNodeType::Image, "image", false, |_| {})?;
    let mid_y = CARD_Y + default_pixels(spec.height) / 2;
    let right_edge = CARD_X + default_pixels(spec.width) - 2;
    for x in [CARD_X + 1, right_edge] {
        let pixel = buffer.as_slice()[(mid_y * buffer.width() + x) as usize];
        assert_eq!(
            (pixel.r, pixel.g, pixel.b),
            (0, 255, 0),
            "the image node paints no border at x={x}"
        );
    }
    // The same edge on a prompt card is chrome, proving the probe sees it.
    let prompt = render(&CanvasNodeType::Prompt, "prompt", false, |_| {})?;
    let pixel = prompt.as_slice()[(mid_y * prompt.width() + CARD_X + 1) as usize];
    assert_ne!(
        (pixel.r, pixel.g, pixel.b),
        (0, 255, 0),
        "a normal card paints its border there"
    );
    Ok(())
}

/// The probe's field: an image node paints no chrome at rest, so anything the
/// body does not cover stays this exact green.
fn is_probe_field(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r < 12 && pixel.g > 240 && pixel.b < 12
}

/// One tolerant colour probe for "neutral ink" (a hairline or a grey glyph)
/// over the plate's greenish glass.
fn is_neutral_ink(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r.abs_diff(pixel.g) <= 25 && pixel.b.abs_diff(pixel.g) <= 35 && pixel.r < 245
}

/// The accent ink of the Generating label (#D85C3A in the light theme).
fn is_accent_ink(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r > 140 && pixel.r.saturating_sub(pixel.g) > 40 && pixel.r.saturating_sub(pixel.b) > 40
}

/// The danger ink of the error line (#b91c1c in the light theme).
fn is_danger_ink(pixel: slint::Rgba8Pixel) -> bool {
    pixel.r > 110 && pixel.r.saturating_sub(pixel.g) > 60 && pixel.r.saturating_sub(pixel.b) > 60
}

/// A card's centre in probe pixels (`CARD_X/CARD_Y` plus half the spec size).
fn card_centre(node_type: &CanvasNodeType) -> (u32, u32) {
    let spec = get_node_spec(node_type);
    (
        CARD_X + default_pixels(spec.width) / 2,
        CARD_Y + default_pixels(spec.height) / 2,
    )
}

/// A generating image node is a designed surface: a frosted plate at the
/// body's normal inset with one centred spinner-over-label group. The plate is
/// what the card paints instead of a border and a fill.
#[test]
fn loading_image_card_paints_a_centred_plate() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Image);
    let buffer = render(&CanvasNodeType::Image, "image", false, |ui| {
        ui.set_probe_status("loading".into());
    })?;
    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let (cx, cy) = card_centre(&CanvasNodeType::Image);
    let pixel = |x: u32, y: u32| buffer.as_slice()[(y * buffer.width() + x) as usize];

    // The plate paints inside the card's space-4 inset; the corner outside it
    // is still the bare canvas (an image node has no chrome of its own).
    assert!(
        !is_probe_field(pixel(left + 24, top + 24)),
        "the loading plate paints inside the card"
    );
    assert!(
        is_probe_field(pixel(left + 4, top + 4)),
        "the plate keeps the body's normal inset"
    );

    // The spinner ring is neutral ink, the label is accent ink. Both are
    // measured inside the plate so the plate's own hairline cannot stand in
    // for either.
    let ring = bbox_where(&buffer, left + 22, top + 22, right - 22, cy, is_neutral_ink)
        .ok_or("the spinner ring paints")?;
    let label = bbox_where(
        &buffer,
        left + 22,
        cy,
        right - 22,
        bottom - 22,
        is_accent_ink,
    )
    .ok_or("the Generating label paints")?;
    assert!(
        (34..=46).contains(&(ring.2 - ring.0)) && (34..=46).contains(&(ring.3 - ring.1)),
        "the spinner is a 40px ring ({}x{})",
        ring.2 - ring.0,
        ring.3 - ring.1
    );
    assert!(
        u32::midpoint(ring.0, ring.2).abs_diff(cx) <= 3,
        "the spinner is horizontally centred ({} vs {cx})",
        u32::midpoint(ring.0, ring.2)
    );
    assert!(
        u32::midpoint(label.0, label.2).abs_diff(cx) <= 4,
        "the label is horizontally centred ({} vs {cx})",
        u32::midpoint(label.0, label.2)
    );
    assert!(
        label.2 - label.0 > 20,
        "the Generating label paints its text ({}px wide)",
        label.2 - label.0
    );
    assert!(
        ring.3 <= label.1,
        "the spinner sits directly above the label ({} vs {})",
        ring.3,
        label.1
    );

    // The whole group (spinner plus label) is centred on the card.
    let group = bbox_where(&buffer, left + 22, top + 22, right - 22, bottom - 22, |p| {
        is_neutral_ink(p) || is_accent_ink(p)
    })
    .ok_or("the loading group paints")?;
    assert!(
        u32::midpoint(group.1, group.3).abs_diff(cy) <= 5,
        "the spinner/label group is vertically centred ({} vs {cy})",
        u32::midpoint(group.1, group.3)
    );

    // The same plate is behind a loading image stack (its card has a fill, so
    // compare the plate against the card's own corner instead).
    let stack = render(&CanvasNodeType::ImageStack, "image-stack", false, |ui| {
        ui.set_probe_status("loading".into());
    })?;
    let stack_pixel = |x: u32, y: u32| stack.as_slice()[(y * stack.width() + x) as usize];
    assert_ne!(
        (
            stack_pixel(left + 24, top + 24).r,
            stack_pixel(left + 24, top + 24).g,
            stack_pixel(left + 24, top + 24).b
        ),
        (
            stack_pixel(left + 4, top + 4).r,
            stack_pixel(left + 4, top + 4).g,
            stack_pixel(left + 4, top + 4).b
        ),
        "a loading image-stack card paints the plate too"
    );
    Ok(())
}

/// A generation in flight paints only the loading surface: a bitmap left on the
/// row (a regeneration, or a stale row rebuild) must never show through the
/// plate. Completion with the same bitmap paints it; failure keeps the error
/// surface clean too.
#[test]
fn loading_and_error_cards_never_paint_a_previous_bitmap() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Image);
    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let red = |buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>| {
        let mut count = 0usize;
        for y in top..bottom.min(buffer.height()) {
            for x in left..right.min(buffer.width()) {
                let pixel = buffer.as_slice()[(y * buffer.width() + x) as usize];
                if pixel.r > 170 && pixel.g < 90 && pixel.b < 90 {
                    count += 1;
                }
            }
        }
        count
    };

    for status in ["loading", "error"] {
        let buffer = render(&CanvasNodeType::Image, "image", false, |ui| {
            ui.set_probe_status(status.into());
            ui.set_content_image(red_image());
        })?;
        assert!(
            red(&buffer) < 1_000,
            "a {status} image card never paints the stale bitmap ({} red px)",
            red(&buffer)
        );
    }

    let ready = render(&CanvasNodeType::Image, "image", false, |ui| {
        ui.set_content_image(red_image());
    })?;
    assert!(
        red(&ready) > 1_000,
        "the completed card paints the result bitmap"
    );
    Ok(())
}

/// An image node with no bitmap is a frosted placeholder, not a transparent
/// hole: the plate, a large muted glyph and the one placeholder line.
#[test]
fn empty_image_card_paints_its_placeholder() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Image);
    let buffer = render(&CanvasNodeType::Image, "image", false, |_| {})?;
    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let (cx, _) = card_centre(&CanvasNodeType::Image);
    let pixel = |x: u32, y: u32| buffer.as_slice()[(y * buffer.width() + x) as usize];

    assert!(
        !is_probe_field(pixel(left + 24, top + 24)),
        "the empty image node paints its frosted plate"
    );
    assert!(
        is_probe_field(pixel(left + 4, top + 4)),
        "the plate keeps the body's normal inset"
    );

    let ink = bbox_where(
        &buffer,
        left + 20,
        top + 20,
        right - 20,
        bottom - 20,
        is_neutral_ink,
    )
    .ok_or("the empty placeholder glyph and line paint")?;
    assert!(
        u32::midpoint(ink.0, ink.2).abs_diff(cx) <= 5,
        "the placeholder column is centred ({} vs {cx})",
        u32::midpoint(ink.0, ink.2)
    );
    assert!(
        ink.3 - ink.1 > 30,
        "the glyph and its line are both painted ({}px tall)",
        ink.3 - ink.1
    );
    Ok(())
}

/// An error image node reuses the plate: a centred, readable message with the
/// retry affordance still below it.
#[test]
fn error_image_card_paints_a_centred_message_and_retry() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::Image);
    let buffer = render(&CanvasNodeType::Image, "image", false, |ui| {
        ui.set_probe_status("error".into());
        ui.set_probe_error("Out of credits".into());
    })?;
    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let (cx, cy) = card_centre(&CanvasNodeType::Image);
    let pixel = |x: u32, y: u32| buffer.as_slice()[(y * buffer.width() + x) as usize];

    assert!(
        !is_probe_field(pixel(left + 24, top + 24)),
        "the error plate paints inside the card"
    );
    assert!(
        is_probe_field(pixel(left + 4, top + 4)),
        "the error plate keeps the body's normal inset"
    );

    let message = bbox_where(&buffer, left, top, right, cy, is_danger_ink)
        .ok_or("the error message paints")?;
    assert!(
        u32::midpoint(message.0, message.2).abs_diff(cx) <= 4,
        "the error message is centred ({} vs {cx})",
        u32::midpoint(message.0, message.2)
    );

    // The retry chip's dark label sits below the card's centre line.
    let retry = bbox_where(&buffer, left, cy, right, bottom, |p| {
        u32::from(p.r) + u32::from(p.g) + u32::from(p.b) < 400
    })
    .ok_or("the retry chip paints below the message")?;
    assert!(
        u32::midpoint(retry.0, retry.2).abs_diff(cx) <= 8,
        "the retry chip is centred ({} vs {cx})",
        u32::midpoint(retry.0, retry.2)
    );
    Ok(())
}

/// Image and Smart Canvas nodes are pure pictures/outputs: no card fill, border
/// or inset at rest. The outline is only the selection/related/drop/hover state.
#[test]
fn image_nodes_paint_only_their_bitmap() {
    let card = include_str!("../ui/components/node-card.slint");
    assert!(
        card.contains(
            "property <bool> frameless: root.kind == \"image\" || root.kind == \"smart-canvas\";"
        ),
        "image and smart-canvas nodes are marked as chrome-less"
    );
    assert!(
        card.contains("border-width: root.frameless ? (root.frameless-outline ? 2px * root.visual-scale : 0px)"),
        "a frameless node has no border unless the outline state is active"
    );
    assert!(
        card.contains("background: root.frameless ? transparent"),
        "a frameless node has no card fill"
    );
    assert!(
        card.contains("padding: root.frameless ? 0px : Theme.space-4 * root.visual-scale;"),
        "a frameless node has no body inset"
    );
    assert!(
        card.contains("property <bool> frameless-outline: root.selected || root.connect-target || root.drop-target")
            && card.contains("|| root.related || node-hit.has-hover;"),
        "selection, related, drop and hover still show the outline"
    );
}

/// Item 7: the prompt node is called "Image Prompt" everywhere the user sees it
/// (spec title and dock/create label), while its type id stays `prompt`.
#[test]
fn the_prompt_node_is_an_image_prompt() {
    let spec = get_node_spec(&CanvasNodeType::Prompt);
    assert_eq!(spec.title, "Image Prompt");
    assert_eq!(
        CanvasNodeType::Prompt.as_str(),
        "prompt",
        "the type id is stable"
    );
    assert!(
        include_str!("../ui/components/icons.slint")
            .contains("type-id == \"prompt\" ? \"Image Prompt\""),
        "the dock/create label follows the spec"
    );
}

/// Item 8: the reference labels are a row of chips at the TOP of the prompt
/// card and nothing else: a chip drop never inserts a name into the prompt text,
/// and there is no second reference pill row under the body.
#[test]
fn reference_chips_sit_above_the_prompt_input_and_never_edit_the_text() {
    let card = include_str!("../ui/components/node-card.slint");
    let prompt_branch = card
        .find("(root.kind == \"prompt\" || root.kind == \"video-prompt\")")
        .unwrap_or(0);
    let chips = card.find("ref-chip-row := HorizontalLayout").unwrap_or(0);
    let input = card.find("prompt-text := Rectangle").unwrap_or(0);
    assert!(prompt_branch > 0 && chips > 0 && input > 0);
    assert!(
        prompt_branch < chips && chips < input,
        "the chip row is above the prompt input"
    );
    assert!(
        !card.contains("node-ref-insert"),
        "a chip drop cannot request a text insertion"
    );
    assert!(
        !card.contains("root.refs"),
        "the prompt card has no second reference pill row at the bottom"
    );
    assert!(
        !card.contains("node-chip-remove"),
        "a chip is not removed through the prompt text"
    );
    assert!(
        !card.contains("drag-ta := TouchArea"),
        "the reference chip row has no drop handler to insert text"
    );
}

// ---- Smart Canvas: create, open/back, preview and persistence ---------------

/// A canvas opened on a temp store plus one Smart Canvas card and the Image
/// Studio project it created.
struct SmartFixture {
    dir: tempfile::TempDir,
    ui: crate::AppWindow,
    app: std::rc::Rc<std::cell::RefCell<crate::app::App>>,
    canvas_id: String,
    node_id: String,
    project_id: String,
}

fn smart_fixture() -> Result<SmartFixture, Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let store = crate::store::Store::at(dir.path().to_path_buf());
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        if app.library.selected_group.is_none() {
            app.library_create_group(&ui);
        }
        app.library_create_project(&ui);
    });
    let canvas_id = crate::app::with_app_r(&app, |app| {
        app.editor
            .as_ref()
            .expect("the canvas project opens")
            .id
            .clone()
    })
    .expect("app is borrowable");
    ui.invoke_create_node("smart-canvas".into());
    let (node_id, project_id) = crate::app::with_app_r(&app, |app| {
        let node = app
            .doc
            .nodes()
            .iter()
            .find(|node| node.node_type == CanvasNodeType::SmartCanvas)
            .expect("the dock created a Smart Canvas node");
        (
            node.id.clone(),
            node.metadata
                .as_ref()
                .and_then(|meta| meta.smart_canvas_project_id.clone())
                .expect("the node carries its project id"),
        )
    })
    .expect("app is borrowable");
    Ok(SmartFixture {
        dir,
        ui,
        app,
        canvas_id,
        node_id,
        project_id,
    })
}

/// `rows` for one node through the model installed on the window.
fn row_of(app: &crate::app::App, id: &str) -> Option<crate::NodeItem> {
    (0..app.rows.row_count())
        .filter_map(|index| app.rows.row_data(index))
        .find(|row| row.id == id)
}

/// Creating a Smart Canvas runs the library's New Image factory exactly once,
/// names the card after the project and links it through metadata; the project
/// is a normal Image library row.
#[test]
fn creating_a_smart_canvas_makes_one_linked_image_project() -> TestResult {
    let fixture = smart_fixture()?;
    crate::app::with_app(&fixture.app, |app| {
        let image_rows: Vec<_> = app
            .library
            .index
            .projects
            .iter()
            .filter(|row| crate::image::is_image_kind(&row.kind))
            .collect();
        assert_eq!(image_rows.len(), 1, "exactly one image project exists");
        let row = image_rows[0];
        assert_eq!(row.id, fixture.project_id, "the card links that project");

        let node = app.doc.node(&fixture.node_id).expect("node survives");
        assert_eq!(node.node_type, CanvasNodeType::SmartCanvas);
        assert_eq!(node.title, row.title, "the card is named after the project");
        assert_eq!(
            node.metadata
                .as_ref()
                .and_then(|meta| meta.smart_canvas_project_id.as_deref()),
            Some(fixture.project_id.as_str())
        );

        // The linked project is stored and indexed, but it is not a home row:
        // image projects only exist behind Smart Canvas cards now.
        app.sync_library(&fixture.ui);
        assert!(
            app.library.index.project(&fixture.project_id).is_some(),
            "the linked image project stays in the index for its card"
        );
        assert!(
            !app.filtered_project_ids().contains(&fixture.project_id),
            "the image project is not listed as a library row"
        );

        // The card is one row of the canvas and is display-only.
        let row = row_of(app, &fixture.node_id).expect("row exists");
        assert_eq!(row.kind.as_str(), "smart-canvas");
        assert!(!row.has_source && !row.has_target, "no connection handles");
    });
    Ok(())
}

/// Double-click enters the Image Studio with the linked project, and the
/// studio's Home returns to the origin canvas (a normal library open still
/// returns to the library).
#[test]
fn smart_canvas_double_click_opens_the_studio_and_home_returns_to_the_canvas() -> TestResult {
    let fixture = smart_fixture()?;
    crate::app::with_app(&fixture.app, |app| {
        app.node_double_click(&fixture.ui, &fixture.node_id);
        assert_eq!(app.view, crate::app::View::Image);
        assert_eq!(
            app.image.meta.as_ref().map(|meta| meta.id.as_str()),
            Some(fixture.project_id.as_str())
        );
        assert_eq!(
            app.image.origin_canvas.as_deref(),
            Some(fixture.canvas_id.as_str()),
            "the origin canvas is remembered"
        );
        assert_eq!(fixture.ui.get_image_board().as_str(), "640");
        assert!(
            fixture.ui.get_image_canvas_image().size().width > 0,
            "the board composite renders in the studio"
        );
    });

    crate::app::with_app(&fixture.app, |app| app.go_home(&fixture.ui));
    crate::app::with_app(&fixture.app, |app| {
        assert_eq!(app.view, crate::app::View::Editor, "back is the canvas");
        assert_eq!(
            app.editor.as_ref().map(|meta| meta.id.as_str()),
            Some(fixture.canvas_id.as_str())
        );
        assert!(app.doc.node(&fixture.node_id).is_some());
    });

    // A studio opened from the library keeps today's behaviour.
    crate::app::with_app(&fixture.app, |app| {
        app.library_open_project(&fixture.ui, &fixture.project_id);
        assert_eq!(app.view, crate::app::View::Image);
        assert!(app.image.origin_canvas.is_none());
        app.go_home(&fixture.ui);
        assert_eq!(app.view, crate::app::View::Library);
        assert!(app.editor.is_none());
    });
    drop(fixture);
    Ok(())
}

/// The card paints the project composite once its board has pixels, and the
/// placeholder otherwise; deleting the project degrades to the placeholder
/// without touching the node.
#[test]
fn smart_canvas_card_previews_the_project_and_falls_back_to_the_placeholder() -> TestResult {
    let fixture = smart_fixture()?;
    crate::app::with_app(&fixture.app, |app| {
        let row = row_of(app, &fixture.node_id).expect("row");
        assert!(row.content_empty, "a fresh project shows the placeholder");

        // Paint the project's pixel layer through the store, the way a studio
        // save does, then let the canvas rebuild its rows.
        let mut file = app.store.load_image(&fixture.project_id).expect("file");
        let layer_id = file.project.board_layers[0].id.clone();
        let mut surface = open_canvas_ps::Surface::new(
            crate::vm::i2u(crate::vm::f2i(file.project.width.round())),
            crate::vm::i2u(crate::vm::f2i(file.project.height.round())),
        );
        surface.fill([200, 30, 30, 255]);
        file.project.board_layers[0].storage_key = Some(format!("bitmap:{layer_id}"));
        let mut bitmaps = std::collections::HashMap::new();
        bitmaps.insert(format!("bitmap:{layer_id}"), surface);
        let painted = crate::image::ImageFile::from_state(
            &file.project,
            &bitmaps,
            &crate::image::assets::ImageAssets::default(),
        )
        .expect("encode");
        app.store.save_image(&painted).expect("save");
        app.sync_rows_full(&fixture.ui);

        let row = row_of(app, &fixture.node_id).expect("row");
        assert!(!row.content_empty, "the preview replaces the placeholder");
        assert!(
            row.content_image.size().width > 0,
            "the card carries the composite"
        );

        // Delete the project: the node survives and shows the placeholder.
        app.library_project_action(&fixture.ui, &fixture.project_id, "delete");
        app.library_project_action(&fixture.ui, &fixture.project_id, "delete");
        app.sync_rows_full(&fixture.ui);
        assert!(
            app.doc.node(&fixture.node_id).is_some(),
            "the node survives"
        );
        let row = row_of(app, &fixture.node_id).expect("row");
        assert!(row.content_empty, "a missing project is the placeholder");
    });
    drop(fixture);
    Ok(())
}

/// A saved canvas reopens with the card and its project link, and the linked
/// project still opens from the library after a restart.
#[test]
fn smart_canvas_survives_a_save_reload_and_the_project_opens_from_the_library() -> TestResult {
    let fixture = smart_fixture()?;
    crate::app::with_app(&fixture.app, |app| {
        app.save_now().expect("canvas save");
    });
    let canvas_id = fixture.canvas_id.clone();
    let node_id = fixture.node_id.clone();
    let project_id = fixture.project_id.clone();

    let app = crate::app::App::new(crate::store::Store::at(fixture.dir.path().to_path_buf()));
    let project = app.store.load_project(&canvas_id)?;
    let node = project
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .expect("the card reloads");
    assert_eq!(node.node_type, CanvasNodeType::SmartCanvas);
    assert_eq!(
        node.metadata
            .as_ref()
            .and_then(|meta| meta.smart_canvas_project_id.as_deref()),
        Some(project_id.as_str())
    );
    assert!(
        !node.title.is_empty(),
        "the card keeps the title it was named with"
    );

    let ui = crate::AppWindow::new()?;
    let app = std::rc::Rc::new(std::cell::RefCell::new(app));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.sync_all(&ui);
        app.library_open_project(&ui, &project_id);
        assert_eq!(app.view, crate::app::View::Image);
        assert_eq!(
            app.image.meta.as_ref().map(|meta| meta.id.as_str()),
            Some(project_id.as_str())
        );
        assert!(app.image.origin_canvas.is_none());
    });
    drop(fixture);
    Ok(())
}

/// Deleting the card leaves the Image Studio project untouched in the library.
#[test]
fn deleting_a_smart_canvas_card_keeps_its_project() -> TestResult {
    let fixture = smart_fixture()?;
    crate::app::with_app(&fixture.app, |app| {
        app.delete_selection(&fixture.ui);
        assert!(
            app.doc.node(&fixture.node_id).is_none(),
            "the card is deleted"
        );
        assert!(
            app.library.index.project(&fixture.project_id).is_some(),
            "the project stays in the library"
        );
        assert!(
            app.store.load_image(&fixture.project_id).is_ok(),
            "the project file stays on disk"
        );
    });
    drop(fixture);
    Ok(())
}

/// A Smart Canvas card body has a preview state and a placeholder state; both
/// paint inside the card.
#[test]
fn smart_canvas_card_renders_preview_and_placeholder() -> TestResult {
    assert_card_fits(
        "smart-canvas preview",
        &CanvasNodeType::SmartCanvas,
        "smart-canvas",
        |ui| ui.set_content_image(red_image()),
    )?;
    assert_card_fits(
        "smart-canvas placeholder",
        &CanvasNodeType::SmartCanvas,
        "smart-canvas",
        |_| {},
    )?;
    let empty = render(&CanvasNodeType::SmartCanvas, "smart-canvas", false, |_| {})?;
    let filled = render(&CanvasNodeType::SmartCanvas, "smart-canvas", false, |ui| {
        ui.set_content_image(red_image());
    })?;
    assert!(
        crate::test_support::differing_pixels(&empty, &filled)? > 500,
        "the preview replaces the placeholder art"
    );
    Ok(())
}

/// A Smart Canvas card is an output, not a framed card: the project composite is
/// the node body, edge to edge (the Image-node treatment). With nothing on the
/// board the body is a plain white plate carrying one quiet centred line only -
/// no card border, no frosted plate, no icon.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one render probe walks edge, plate, title and source in one pass"
)]
fn smart_canvas_card_is_a_frameless_output() -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(&CanvasNodeType::SmartCanvas);
    let left = CARD_X;
    let top = CARD_Y;
    let right = left + default_pixels(spec.width);
    let bottom = top + default_pixels(spec.height);
    let (cx, cy) = card_centre(&CanvasNodeType::SmartCanvas);
    let pixel = |buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, x: u32, y: u32| {
        buffer.as_slice()[(y * buffer.width() + x) as usize]
    };
    // Probe the body away from the 18px rounded corners: the edge midpoints say
    // whether a border paints, the inner points whether the plate is inset.
    let edge = [
        (cx, top),
        (cx, top + 1),
        (cx, bottom - 1),
        (left, cy),
        (left + 1, cy),
        (right - 1, cy),
        (left + 24, top + 1),
        (right - 25, top + 1),
        (left + 24, bottom - 2),
        (right - 25, bottom - 2),
    ];
    let empty = render(&CanvasNodeType::SmartCanvas, "smart-canvas", false, |_| {})?;
    for (x, y) in edge {
        let p = pixel(&empty, x, y);
        assert!(
            p.r == 255 && p.g == 255 && p.b == 255,
            "the empty body is plain white edge to edge at ({x},{y}): {:?}",
            (p.r, p.g, p.b)
        );
    }

    // One quiet centred line: no glyph above it, nothing else on the plate.
    let line = bbox_where(
        &empty,
        left + 2,
        top + 2,
        right - 2,
        bottom - 2,
        is_neutral_ink,
    )
    .ok_or("the empty line paints")?;
    assert!(
        u32::midpoint(line.0, line.2).abs_diff(cx) <= 4,
        "the empty line is centred ({} vs {cx})",
        u32::midpoint(line.0, line.2)
    );
    assert!(
        u32::midpoint(line.1, line.3).abs_diff(cy) <= 6,
        "the empty line sits on the body's centre line ({} vs {cy})",
        u32::midpoint(line.1, line.3)
    );
    assert!(
        line.3 - line.1 <= 24 && line.2 - line.0 <= 200,
        "the plate carries one small line, no icon above it ({}x{}px)",
        line.2 - line.0,
        line.3 - line.1
    );

    // The title still sits above the card, as for every other node.
    let title = bbox_where(&empty, left, top - 30, right, top - 2, |p| {
        !is_probe_field(p)
            && p.r.abs_diff(74) <= 12
            && p.g.abs_diff(68) <= 12
            && p.b.abs_diff(60) <= 12
    })
    .ok_or("the title paints above the card")?;
    assert!(
        title.3 - title.1 <= 20 && title.0 >= left,
        "the title keeps its slot above the body ({title:?})"
    );

    // With pixels on the board the composite is the body: no border, no inset.
    let filled = render(&CanvasNodeType::SmartCanvas, "smart-canvas", false, |ui| {
        ui.set_content_image(red_image());
    })?;
    for (x, y) in edge {
        let p = pixel(&filled, x, y);
        assert!(
            p.r > 180 && p.g < 60 && p.b < 60,
            "the composite fills the node rect at ({x},{y}): {:?}",
            (p.r, p.g, p.b)
        );
    }

    // Source guard: the empty branch is the white plate + one line, and the
    // card holds no frosted plate or glyph for this kind.
    let card = card_source();
    let start = card
        .find("if root.status == \"\" && root.kind == \"smart-canvas\"")
        .ok_or("the smart-canvas body branch exists")?;
    let tail = card[start..]
        .find("// Video body")
        .map(|offset| start + offset)
        .ok_or("the smart-canvas branch ends before the video body")?;
    let branch = &card[start..tail];
    for needle in [
        "Nothing here yet",
        "background: root.content-empty ? #FFFFFF : transparent;",
    ] {
        assert!(
            branch.contains(needle),
            "the empty body is a white plate ({needle})"
        );
    }
    for forbidden in ["FrostedPlate", "Icons.", "Path {", "border-width"] {
        assert!(
            !branch.contains(forbidden),
            "the smart-canvas body is not a framed card ({forbidden})"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Zoom invariance: the generation body is one design at every visual scale
// ---------------------------------------------------------------------------

/// Vertical runs of label-column ink, merged into bands. The first band is the
/// model row's brand badge (that row has no label column), the rest are the
/// parameter labels in row order.
fn label_bands(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    scale: f64,
    spec_height: f64,
) -> Vec<(u32, u32)> {
    let left = CARD_X;
    let top = CARD_Y;
    let inner_left = left + px(16.0 * scale);
    let x1 = inner_left + px(44.0 * scale);
    let y0 = top + px(16.0 * scale);
    let y1 = top + px(spec_height * scale) - px(16.0 * scale);
    let dark = |p: slint::Rgba8Pixel| p.r < 180 && p.g < 180 && p.b < 180;
    let mut bands = Vec::new();
    let mut start: Option<u32> = None;
    let mut previous = 0u32;
    for y in y0..y1 {
        let painted = (inner_left + 2..x1)
            .any(|x| dark(buffer.as_slice()[(y * buffer.width() + x) as usize]));
        match (painted, start) {
            (true, None) => {
                start = Some(y);
                previous = y;
            }
            (true, Some(_)) => previous = y,
            (false, Some(first)) => {
                bands.push((first, previous + 1));
                start = None;
            }
            (false, None) => {}
        }
    }
    if let Some(first) = start {
        bands.push((first, previous + 1));
    }
    bands.into_iter().filter(|(a, b)| b > a).collect()
}

/// Brand fill of the Generate button, measured only in the card's bottom band
/// so the selected parameter chips cannot be mistaken for it.
fn generation_button(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    scale: f64,
    spec_height: f64,
) -> Option<(u32, u32, u32, u32)> {
    let left = CARD_X + px(16.0 * scale);
    let top = CARD_Y + px(spec_height * scale) - px(60.0 * scale);
    let right = CARD_X + px(320.0 * scale) - px(16.0 * scale);
    let bottom = CARD_Y + px(spec_height * scale);
    bbox_where(buffer, left, top, right, bottom, is_brand)
}

/// Headless probes at 0.5x/1x/2x/3x: the card rect and `visual-scale` grow by
/// the same factor, the five label/model bands keep their normalised positions,
/// nothing paints outside the card, and the Generate button's inset/height stay
/// the same fractions of the card.
fn assert_generation_card_is_zoom_invariant(
    label: &str,
    node_type: &CanvasNodeType,
    kind: &str,
    mode: &str,
) -> TestResult {
    init_backend_allow_existing();
    let spec = get_node_spec(node_type);
    let spec_w = spec.width;
    let spec_h = spec.height;
    let scales = [0.5_f64, 1.0, 2.0, 3.0];
    let mut baseline: Option<Vec<f64>> = None;

    for &scale in &scales {
        let actual = render_at(node_type, kind, scale, false, |ui| {
            ui.set_can_generate(true);
            ui.set_mode(mode.into());
        })?;
        let bare = render_at(node_type, kind, scale, true, |_| {})?;
        let width = px(spec_w * scale);
        let height = px(spec_h * scale);
        let right = CARD_X + width;
        let bottom = CARD_Y + height;
        let window_w = px(spec_w * scale) + CARD_X * 2;
        let window_h = px(spec_h * scale) + CARD_Y + 40;

        // Nothing the body paints may leave the card (handles are identical on
        // `bare`, so they cancel out in the diff).
        let below = diff_rect(&actual, &bare, 0, bottom, window_w, window_h);
        let beside = diff_rect(&actual, &bare, right, CARD_Y, window_w, window_h);
        let above = diff_rect(&actual, &bare, 0, 0, window_w, CARD_Y);
        let left_of = diff_rect(&actual, &bare, 0, CARD_Y, CARD_X, bottom);
        assert_eq!(below, 0, "{label} @{scale}x: body paints below the card");
        assert_eq!(beside, 0, "{label} @{scale}x: body paints beside the card");
        assert_eq!(above, 0, "{label} @{scale}x: body paints above the card");
        assert_eq!(
            left_of, 0,
            "{label} @{scale}x: body paints left of the card"
        );

        // Five bands in the same order at every zoom: model badge, then the
        // four parameter rows (image: Quality/Size/Res/Count; video:
        // Res/Seconds/Audio/Mark).
        let bands = label_bands(&actual, scale, spec_h);
        assert_eq!(
            bands.len(),
            5,
            "{label} @{scale}x: five bands, got {bands:?}"
        );
        let normalised: Vec<f64> = bands
            .iter()
            .map(|&(a, b)| {
                (f64::midpoint(f64::from(a), f64::from(b)) - f64::from(CARD_Y)) / f64::from(height)
            })
            .collect();
        if let Some(expected) = &baseline {
            for (index, (got, want)) in normalised.iter().zip(expected).enumerate() {
                assert!(
                    (got - want).abs() < 0.012,
                    "{label} @{scale}x: band {index} at {got:.3}, 1x at {want:.3}"
                );
            }
        } else {
            baseline = Some(normalised);
        }

        // The action is pinned to the bottom inset: left/right are 5% of the
        // card width, the bottom gap and height are the same fractions at every
        // zoom (16px inset / 36px height in the default 320-wide design).
        let button = generation_button(&actual, scale, spec_h)
            .ok_or_else(|| format!("{label} @{scale}x: the Generate button paints"))?;
        let inset_left = f64::from(button.0 - CARD_X) / f64::from(width);
        let inset_right = f64::from(right - button.2) / f64::from(width);
        let inset_bottom = f64::from(bottom - button.3) / f64::from(height);
        let button_height = f64::from(button.3 - button.1) / f64::from(height);
        assert!(
            (inset_left - 0.05).abs() < 0.012,
            "{label} @{scale}x: left inset {inset_left:.3}"
        );
        assert!(
            (inset_right - 0.05).abs() < 0.012,
            "{label} @{scale}x: right inset {inset_right:.3}"
        );
        assert!(
            (inset_bottom - 16.0 / spec_h).abs() < 0.012,
            "{label} @{scale}x: bottom inset {inset_bottom:.3}"
        );
        assert!(
            (button_height - 36.0 / spec_h).abs() < 0.012,
            "{label} @{scale}x: button height {button_height:.3}"
        );
    }
    Ok(())
}

#[test]
fn image_generation_card_is_zoom_invariant() -> TestResult {
    assert_generation_card_is_zoom_invariant(
        "image-generation",
        &CanvasNodeType::ImageGeneration,
        "image-generation",
        "image",
    )
}

#[test]
fn video_generation_card_is_zoom_invariant() -> TestResult {
    assert_generation_card_is_zoom_invariant(
        "video-generation",
        &CanvasNodeType::VideoGeneration,
        "video-generation",
        "video",
    )
}

/// The generation panel has no unscaled pixel length: every visual length is a
/// design constant multiplied by `vs`. This guards the rows, chips, badge,
/// fonts, slider and borders against one zoom-specific constant creeping back.
#[test]
fn generation_panel_lengths_all_scale_with_vs() {
    let panel = include_str!("../ui/components/panel-popovers.slint").replace("\r\n", "\n");
    for line in panel.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let is_length = matches!(
            key,
            "height"
                | "min-height"
                | "max-height"
                | "width"
                | "min-width"
                | "max-width"
                | "border-width"
                | "border-radius"
                | "font-size"
                | "padding"
                | "padding-left"
                | "padding-right"
                | "spacing"
                | "stroke-width"
                | "track-height"
                | "track-radius"
                | "handle-size"
        );
        if !is_length {
            continue;
        }
        let value = value.trim().trim_end_matches(';').trim();
        if value == "0px"
            || value.ends_with('%')
            || value.starts_with("Theme.")
            || value.starts_with("root.")
            || value.starts_with("self.")
            || value.starts_with("parent.")
            || value.starts_with("min(")
            || value.starts_with("max(")
        {
            continue;
        }
        assert!(
            value.contains("* root.vs") || value.contains("* vs"),
            "panel length ignores vs: `{line}`"
        );
    }

    // The SVG play glyph's stroke is a visual length too.
    let card = include_str!("../ui/components/node-card.slint");
    assert!(
        card.contains("stroke-width: 2px * root.visual-scale;"),
        "the Generate play glyph's stroke scales with the card"
    );
}

/// Regression: a prompt row born with its text already stored (a restored
/// document, or a row built after the edit landed) used to open its editor with
/// an empty buffer. `changed node-id` / `changed body` never fire for a value
/// that is already there when the card is created, so the `TextInput` stayed
/// empty and the card had nothing to paint.
#[test]
fn a_prompt_with_stored_text_reopens_and_paints_it_after_double_click() -> TestResult {
    /// Runs the real UI path (double-click to edit, typed `edited`, blur to
    /// commit) and then the real double-click route *before* the window is
    /// shown, so the card is created with its text already in the row, then
    /// renders the open editor.
    fn render_editor(
        text: &str,
    ) -> Result<crate::test_support::Buffer, Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let ui = crate::AppWindow::new()?;
        let app = std::rc::Rc::new(std::cell::RefCell::new(crate::app::App::new(
            crate::store::Store::at(dir.path().to_path_buf()),
        )));
        crate::events::wire(&ui, &app);
        crate::app::with_app(&app, |app| app.sync_all(&ui));
        ui.invoke_create_node("prompt".into());
        let id = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().first().map(|node| node.id.clone())
        })
        .flatten()
        .expect("the dock creates a prompt node");

        ui.invoke_node_double_click(id.clone().into());
        ui.invoke_node_prompt_edited(
            id.clone().into(),
            text.into(),
            i32::try_from(text.len()).unwrap_or(0),
        );
        ui.invoke_node_prompt_blur(id.clone().into());
        ui.invoke_node_double_click(id.clone().into());

        crate::app::with_app(&app, |app| {
            assert_eq!(
                app.panel.prompt, text,
                "double-click seeds the editor with the stored prompt"
            );
            assert_eq!(
                app.panel.prompt_origin, text,
                "the pre-edit origin is the stored prompt"
            );
            let row = app.rows.row_data(0).expect("the prompt row exists");
            assert_eq!(
                row.body.as_str(),
                text,
                "the row body is still the stored prompt text"
            );
            assert!(row.prompt_editing, "the editor is open");
            assert_eq!(
                app.doc
                    .node(&id)
                    .and_then(|node| node.metadata.as_ref())
                    .and_then(|meta| meta.prompt.as_deref()),
                Some(text),
                "the document keeps exactly the stored prompt text"
            );
        });
        ui.set_view("editor".into());
        ui.show()?;
        ui.window().request_redraw();
        Ok(ui.window().take_snapshot()?)
    }

    init_backend_allow_existing();
    let filled = render_editor("A RED FOX IN SNOW")?;
    let blank = render_editor("")?;
    let differing = crate::test_support::differing_pixels(&filled, &blank)?;
    assert!(
        differing > 50,
        "the reopened editor paints the stored prompt text ({differing} differing pixels)"
    );
    Ok(())
}

/// Regression: typing into a prompt whose row was born with its text must
/// append to that text, never replace it with just the typed character. This
/// also proves the editor write gate is armed for such a row.
#[test]
fn typing_into_a_prompt_row_born_with_text_keeps_the_text() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = crate::AppWindow::new()?;
    let app = std::rc::Rc::new(std::cell::RefCell::new(crate::app::App::new(
        crate::store::Store::at(dir.path().to_path_buf()),
    )));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    ui.invoke_create_node("prompt".into());
    let id = crate::app::with_app_r(&app, |app| {
        app.doc.nodes().first().map(|node| node.id.clone())
    })
    .flatten()
    .ok_or("the dock creates a prompt node")?;
    crate::app::with_app(&app, |app| {
        app.panel_prompt_edit_begin(&ui, &id);
        app.panel_prompt_edited(&ui, "A RED FOX IN SNOW", 17);
        app.panel_prompt_blur(&ui);
    });
    ui.set_view("editor".into());
    ui.show()?;
    crate::app::with_app(&app, |app| app.node_double_click(&ui, &id));

    ui.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "!".into() });

    let stored = crate::app::with_app_r(&app, |app| {
        app.doc
            .node(&id)
            .and_then(|node| node.metadata.as_ref())
            .and_then(|meta| meta.prompt.clone())
    })
    .flatten()
    .ok_or("the prompt node still exists")?;
    assert!(
        stored.contains("A RED FOX IN SNOW"),
        "typing keeps the stored prompt text: {stored:?}"
    );
    assert!(
        stored.contains('!'),
        "the typed character is written: {stored:?}"
    );
    Ok(())
}

#[test]
fn the_prompt_editor_shows_the_existing_text() -> TestResult {
    crate::config_test_support::init_backend()?;
    let blank = render(&CanvasNodeType::Prompt, "prompt", false, |ui| {
        ui.set_probe_prompt_editing(true);
        ui.set_body("".into());
    })?;
    let filled = render(&CanvasNodeType::Prompt, "prompt", false, |ui| {
        ui.set_probe_prompt_editing(true);
        ui.set_body("A RED FOX IN SNOW".into());
    })?;
    let differing = blank
        .as_slice()
        .iter()
        .zip(filled.as_slice())
        .filter(|(a, b)| a.r != b.r || a.g != b.g || a.b != b.b)
        .count();
    assert!(
        differing > 50,
        "the editor paints the existing prompt text ({differing})"
    );
    Ok(())
}

#[test]
fn all_four_node_corners_have_invisible_resize_zones() {
    let src = include_str!("../ui/components/node-handles.slint").replace("\r\n", "\n");
    for corner in ["top-left", "top-right", "bottom-left", "bottom-right"] {
        let needle = format!("root.resize-pointer({corner:?}");
        assert!(src.contains(&needle), "the {corner} corner is draggable");
    }
    assert!(
        !src.contains("resize-grip"),
        "there is no visible resize grip; the corner zone is invisible"
    );
}

#[test]
fn attempt_card_renders_the_1_to_9_version_band() -> TestResult {
    init_backend_allow_existing();
    let buffer = render(
        &CanvasNodeType::ImageAttempt,
        "image-attempt",
        false,
        |ui| {
            ui.set_probe_attempt_version(7);
            ui.set_probe_attempt_title("warmer palette".into());
            ui.set_probe_attempt_reason("Raised the camera and warmed the light".into());
            ui.set_body("A red fox in a snowy bamboo forest".into());
            ui.set_content_image(red_image());
        },
    )?;
    let pixels: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    let image = image::RgbaImage::from_raw(buffer.width(), buffer.height(), pixels)
        .ok_or("attempt card snapshot size")?;
    let path = std::env::temp_dir().join("open-canvas-attempt-card.png");
    image.save(&path)?;
    Ok(())
}

#[test]
fn attempt_card_elides_long_copy() -> TestResult {
    init_backend_allow_existing();
    let buffer = render(
        &CanvasNodeType::ImageAttempt,
        "image-attempt",
        false,
        |ui| {
            ui.set_probe_attempt_version(12);
            ui.set_probe_attempt_title(
                "O11A1 — icon v4 whale with a title that must stay on one line".into(),
            );
            ui.set_probe_attempt_reason(
                "Raised the camera, warmed the light, moved the subject and tightened the crop"
                    .into(),
            );
            ui.set_body(
                "Flat vector software app icon, square 1:1 composition, one rounded-square tile \
                 that fills the frame, only solid flat colors, absolutely no gradients, no glow, \
                 no glass, no 3D, no shading, no shadows, no textures, no tech effects, simple \
                 geometric shapes, crisp clean edges, generous negative space, no text, no letters."
                    .into(),
            );
            ui.set_content_image(red_image());
        },
    )?;
    let pixels: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    let image = image::RgbaImage::from_raw(buffer.width(), buffer.height(), pixels)
        .ok_or("attempt card snapshot size")?;
    let path = std::env::temp_dir().join("open-canvas-attempt-card-long.png");
    image.save(&path)?;
    Ok(())
}
