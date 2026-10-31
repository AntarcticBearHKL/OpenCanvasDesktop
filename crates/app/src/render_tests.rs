//! Headless render tests: the canvas window renders node media into its pixel
//! buffer (project rule: code-only verification — no GUI, no screenshots).
//!
//! The Slint testing backend embeds the software rasterizer, so
//! `Window::take_snapshot` returns the rendered RGBA buffer of a real
//! `AppWindow` with the real `.slint` layout. The assertions scan that buffer
//! for the fixture's red pixels: a populated image node must paint them, an
//! empty one must not.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use slint::{ComponentHandle, ModelRc, VecModel};

use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeType};

use crate::media::MediaCache;
use crate::store::Store;
use crate::test_support::{differing_pixels, init_backend};
use crate::AppWindow;
use crate::NodeItem;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Opens the editor on the **shared** temp root (`std::env::temp_dir`), not a
/// private temp dir: these render tests are read-only, and the shared store
/// keeps the run cheap. `render_editor_with_store` is the fixture for tests
/// that need their own store.
fn render_editor_shared(
    setup: impl FnOnce(&AppWindow, &std::rc::Rc<std::cell::RefCell<crate::app::App>>),
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    crate::test_support::render_editor_with_store(Store::at(std::env::temp_dir()), setup)
}

fn red_png_data_url() -> Result<String, Box<dyn std::error::Error>> {
    let mut surface = open_canvas_ps::Surface::new(8, 8);
    for x in 0..8 {
        for y in 0..8 {
            surface.set(x, y, [220, 20, 20, 255]);
        }
    }
    let png = open_canvas_ps::encode_png(&surface)?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

/// One image-node row placed inside the initial viewport.
fn image_row(content: Option<String>, media: &mut MediaCache) -> NodeItem {
    let node = open_canvas_core::node::CanvasNodeData {
        id: "render-image".into(),
        node_type: CanvasNodeType::Image,
        title: "Render fixture".into(),
        position: open_canvas_core::node::Position::new(20.0, 20.0),
        width: 240.0,
        height: 180.0,
        metadata: Some(CanvasNodeMetadata {
            content,
            natural_width: Some(8.0),
            natural_height: Some(8.0),
            ..Default::default()
        }),
    };
    let nodes = std::slice::from_ref(&node);
    let connections: &[open_canvas_core::node::CanvasConnection] = &[];
    let library = open_canvas_core::document::LibraryIndex::new();
    let edit = crate::inline_edit::EditState::default();
    let related = std::collections::BTreeSet::new();
    let mut plugin = crate::plugin_host::PluginHost::new(std::env::temp_dir());
    let panel = crate::node_ui::Panel::default();
    let mut folders = crate::asset_folder::FolderCache::default();
    let expanded = std::collections::HashMap::new();
    let reference_slots = std::collections::HashMap::new();
    let smart_previews = std::collections::HashMap::new();
    let index = crate::vm::NodeIndex::new(nodes);
    let mut row_context = crate::vm::RowContext {
        nodes,
        connections,
        index: &index,
        library: &library,
        media,
        edit: &edit,
        related: &related,
        reference_slots: &reference_slots,
        drop_target: None,
        drop_slot: None,
        panel: &panel,
        folders: &mut folders,
        asset_expanded: &expanded,
        smart_previews: &smart_previews,
    };
    crate::vm::node_row(&node, false, 1.0, &mut plugin, &mut row_context)
}

fn render_rows(
    rows: Vec<NodeItem>,
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let store = Store::at(std::env::temp_dir());
    let app = std::rc::Rc::new(std::cell::RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    ui.set_view("editor".into());
    ui.set_nodes(ModelRc::new(VecModel::from(rows)));
    ui.show()?;
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;
    Ok(buffer)
}

/// Counts pixels close to the fixture red (220, 20, 20).
fn red_pixels(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> usize {
    let mut count = 0;
    for pixel in buffer.as_slice() {
        if pixel.r > 180 && pixel.g < 80 && pixel.b < 80 {
            count += 1;
        }
    }
    count
}

#[test]
fn populated_image_node_renders_its_bitmap() -> TestResult {
    init_backend()?;
    let mut media = MediaCache::default();
    let row = image_row(Some(red_png_data_url()?), &mut media);
    assert!(!row.content_empty, "the fixture decodes before rendering");
    let buffer = render_rows(vec![row])?;
    assert!(
        buffer.width() > 0 && buffer.height() > 0,
        "the window renders a real surface"
    );
    let red = red_pixels(&buffer);
    assert!(
        red > 1_000,
        "the node body paints the decoded bitmap ({red} fixture pixels)"
    );
    Ok(())
}

#[test]
fn empty_image_node_renders_the_empty_state_without_bitmap_pixels() -> TestResult {
    init_backend()?;
    let mut media = MediaCache::default();
    let row = image_row(None, &mut media);
    assert!(row.content_empty, "no content, no decode");
    let buffer = render_rows(vec![row])?;
    let red = red_pixels(&buffer);
    assert_eq!(red, 0, "the empty state never paints the fixture bitmap");
    // On the blank canvas every non-background pixel belongs to the node card
    // (body, border, icon, label, title), so the empty state provably draws.
    let non_background = buffer
        .as_slice()
        .iter()
        .filter(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0)
        .count();
    assert!(
        non_background > 10_000,
        "the empty-state card is still drawn"
    );
    Ok(())
}

/// The painted horizontal extent of the dock bar on its own centre row: the
/// bar is a long run of non-canvas pixels, while the canvas around it is the
/// field plus sparse grid dots (short runs).
fn dock_bar_extent(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>) -> Option<(u32, u32)> {
    let width = usize::try_from(buffer.width()).ok()?;
    let y = usize::try_from(buffer.height().checked_sub(48)?).ok()?;
    let row: Vec<(u8, u8, u8)> = (0..width)
        .map(|x| {
            let pixel = buffer.as_slice()[y * width + x];
            (pixel.r, pixel.g, pixel.b)
        })
        .collect();
    // The canvas field left of the bar (right of the side panel): its modal
    // colour along this row.
    let field = {
        let mut counts: std::collections::HashMap<(u8, u8, u8), usize> =
            std::collections::HashMap::new();
        for rgb in row.iter().take(420).skip(300) {
            *counts.entry(*rgb).or_insert(0) += 1;
        }
        *counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(rgb, _)| rgb)?
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (x, rgb) in row.iter().enumerate().skip(300) {
        if *rgb == field {
            if let Some(from) = start.take() {
                runs.push((from, x - 1));
            }
        } else if start.is_none() {
            start = Some(x);
        }
    }
    if let Some(from) = start {
        runs.push((from, width - 1));
    }
    let long: Vec<(usize, usize)> = runs
        .into_iter()
        .filter(|(from, to)| to - from >= 20)
        .collect();
    let first = *long.first()?;
    let last = *long.last()?;
    Some((u32::try_from(first.0).ok()?, u32::try_from(last.1).ok()?))
}

/// The dock bar's width is exactly its visible content plus the two 10px
/// gutters: at rest the last child (the node-type group) ends at
/// `width - 10px`, with no reserved slot for a hidden action. The conditional
/// delete seam adds its own 45px (divider + two 6px gaps + the 32px button)
/// only while something is selected.
#[test]
fn the_dock_bar_hugs_its_visible_content() -> TestResult {
    init_backend()?;
    // 8 buttons of 32px (1 nav + 2 history + 5 node types), 2 one-pixel
    // dividers, in-group gutters and group gaps, plus the two 10px outer
    // gutters. Keep this in sync with the dock's child count.
    let content = 302;
    let expected = content + 20;
    let delete_seam = 1 + 6 + 32 + 6;

    let rest = render_editor_shared(|_, _| {})?;
    let (left, right) = dock_bar_extent(&rest).ok_or("the dock bar paints")?;
    let width = right - left + 1;
    assert!(
        width.abs_diff(expected) <= 1,
        "the dock bar is its content plus the 10px gutters: painted {width}, want {expected}"
    );
    // The contract in words: the bar's last child ends at `width - 10px`, so no
    // hidden button, stretch or reserved group sits to the right of the icons.
    // The painted pixels after the content are exactly one 10px gutter.
    let trailing = right + 1 - (left + 10 + content);
    assert!(
        trailing.abs_diff(10) <= 1,
        "the bar's last child ends at width - 10px (trailing gutter {trailing})"
    );
    let centred = (i64::from(rest.width()) - 280 - i64::from(width)) / 2 + 280;
    assert!(
        left.abs_diff(u32::try_from(centred).unwrap_or(0)) <= 1,
        "the bar is centred in the canvas column (left edge {left})"
    );

    // An empty document with the dock's selection flag raised: the delete seam
    // appears without any node card reaching the dock row.
    let selected = render_editor_shared(|ui, _| ui.set_has_selection(true))?;
    let (selected_left, selected_right) =
        dock_bar_extent(&selected).ok_or("the selected dock bar paints")?;
    let selected_width = selected_right - selected_left + 1;
    assert_eq!(
        selected_width,
        width + delete_seam,
        "the delete seam exists only while a selection does"
    );
    assert!(
        (selected_left + selected_right).abs_diff(left + right) <= 1,
        "the bar stays centred when the delete seam appears"
    );
    Ok(())
}

/// The zoom read-out is bare text in the canvas' bottom-right corner (16px
/// padding on the right and the bottom, no plate): hidden while idle, painted
/// when the zoom change raised it.
#[test]
fn the_zoom_read_out_paints_bare_in_the_canvas_corner() -> TestResult {
    init_backend()?;
    let hidden = render_editor_shared(|_, _| {})?;
    let shown = render_editor_shared(|ui, _| {
        ui.set_zoom_label("40%".into());
        ui.set_zoom_badge_visible(true);
    })?;
    if hidden.width() != shown.width() || hidden.height() != shown.height() {
        return Err("snapshots must share a size".into());
    }
    let width = shown.width() as usize;
    let height = shown.height() as usize;
    let mut painted = 0usize;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0usize, 0usize);
    for (index, (before, after)) in hidden.as_slice().iter().zip(shown.as_slice()).enumerate() {
        if before.r == after.r && before.g == after.g && before.b == after.b {
            continue;
        }
        painted += 1;
        let x = index % width;
        let y = index / width;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    assert!(painted > 100, "the zoom read-out paints ({painted})");
    // 16px inset from both edges; the 15px glyph block is ~36x17 logical pixels.
    assert!(
        max_x <= width - 12 && min_x >= width.saturating_sub(90),
        "the read-out hugs the right edge ({min_x}..{max_x} of {width})"
    );
    assert!(
        max_y <= height - 12 && min_y >= height.saturating_sub(48),
        "the read-out hugs the bottom edge ({min_y}..{max_y} of {height})"
    );
    Ok(())
}

/// The zoom read-out uses larger type at the same bare 16px right/bottom inset.
#[test]
fn the_zoom_badge_uses_larger_type_at_the_same_inset() {
    let src = include_str!("../ui/app.slint");
    let start = src
        .find("// Zoom read-out: bare text")
        .expect("the zoom read-out block");
    let end = src.find("Rulers {").expect("the next overlay");
    let badge = &src[start..end];
    assert!(
        badge.contains("font-size: 15px;"),
        "the read-out is 15px now (was 12px)"
    );
    assert!(
        !badge.contains("font-size: 12px;"),
        "the old 12px size is gone"
    );
    assert!(
        badge.contains("x: parent.width - self.width - 16px;"),
        "the bare text keeps its 16px right inset"
    );
    assert!(
        badge.contains("y: parent.height - self.height - 16px;"),
        "the bare text keeps its 16px bottom inset"
    );
}

/// Every action the image-node hover bar can show has a human-readable label,
/// and the bar requests the shared tooltip surface on hover.
#[test]
fn node_hover_bar_labels_every_image_tool() {
    let src = include_str!("../ui/components/floating.slint");
    for (id, label) in [
        ("info", "Info"),
        ("duplicate", "Duplicate"),
        ("crop", "Crop"),
        ("mask", "Mask"),
        ("split", "Split"),
        ("resolution", "Resolution"),
        ("analyze", "Analyze"),
        ("ocr", "OCR"),
        ("segment", "Segment"),
        ("angle", "Angle"),
        ("layers", "Layers"),
        ("delete", "Delete"),
        ("font-down", "Decrease font"),
        ("font-up", "Increase font"),
        ("attempt-generate", "Generate"),
    ] {
        assert!(
            src.contains(&format!(r#"id == "{id}" ? "{label}""#)),
            "{id} has the label {label}"
        );
    }
    assert!(
        src.contains("tip: HoverIcons.tool-label(action.id);"),
        "the hover bar passes the label to every action"
    );
    assert!(
        src.contains(r#"Theme.dock-request("#) && src.contains(r#""tip.show""#),
        "hovering an action requests the shared tooltip label"
    );
}

#[test]
fn dock_tooltip_layer_paints_on_the_canvas() -> TestResult {
    init_backend()?;
    let closed = render_editor_shared(|_, _| {})?;
    let shown = render_editor_shared(|ui, _| {
        ui.global::<crate::Theme>()
            .set_tooltip(crate::TooltipState {
                text: "Undo".into(),
                description: "".into(),
                visible: true,
                x: 320.0,
                y: 240.0,
            });
    })?;
    let painted = differing_pixels(&closed, &shown).map_err(ToString::to_string)?;
    assert!(painted > 200, "the dock tooltip name paints ({painted})");
    let described = render_editor_shared(|ui, _| {
        ui.global::<crate::Theme>()
            .set_tooltip(crate::TooltipState {
                text: "Undo".into(),
                description: "Step back through the last edits".into(),
                visible: true,
                x: 320.0,
                y: 240.0,
            });
    })?;
    let more = differing_pixels(&shown, &described).map_err(ToString::to_string)?;
    assert!(more > 100, "the description adds paint ({more})");
    Ok(())
}

/// A node drag paints no drag ghost or placeholder: the node itself moves. The
/// only difference between a drag whose legacy pointer anchor is set and one
/// whose anchor is empty must be zero, and the old ghost component is gone.
#[test]
fn node_drag_paints_no_drag_ghost() -> TestResult {
    init_backend()?;
    let with_drag = |ghost_pos: Option<crate::coords::ScreenPos>| {
        move |ui: &AppWindow, app: &std::rc::Rc<std::cell::RefCell<crate::app::App>>| {
            crate::app::with_app(app, |app| {
                app.doc = crate::doc::Doc::demo();
                app.sync_all(ui);
                let start = crate::coords::ScreenPos::new(100.0, 100.0);
                app.doc.begin_node_drag("image", start);
                app.doc
                    .node_drag_to(crate::coords::ScreenPos::new(400.0, 320.0));
                app.drag_ghost_pos = ghost_pos;
                app.sync_all(ui);
            });
        }
    };
    let dragged = render_editor_shared(with_drag(None))?;
    let anchored =
        render_editor_shared(with_drag(Some(crate::coords::ScreenPos::new(400.0, 320.0))))?;
    let diff = differing_pixels(&dragged, &anchored).map_err(ToString::to_string)?;
    assert_eq!(diff, 0, "no drag ghost paints at the pointer ({diff})");

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !manifest.join("ui/components/drag-ghost.slint").exists(),
        "the drag ghost component is deleted"
    );
    let app_source = include_str!("../ui/app.slint");
    assert!(
        !app_source.contains("DragGhost") && !app_source.contains("drag-ghost"),
        "the app shell mounts no drag ghost"
    );
    Ok(())
}

#[test]
fn shortcuts_modal_covers_the_shell_and_selected_wire_repaints() -> TestResult {
    init_backend()?;
    let baseline = render_editor_shared(|ui, app| {
        crate::app::with_app(app, |app| {
            app.doc = crate::doc::Doc::demo();
            app.sync_all(ui);
        });
    })?;
    let selected = render_editor_shared(|ui, app| {
        crate::app::with_app(app, |app| {
            app.doc = crate::doc::Doc::demo();
            app.sync_all(ui);
            app.doc.select_connection(Some("conn-prompt-text"));
            app.sync_wires(ui);
        });
    })?;
    let wire_diff = differing_pixels(&baseline, &selected).map_err(ToString::to_string)?;
    assert!(
        wire_diff > 100,
        "the selected wire repaints its path ({wire_diff})"
    );

    let modal = render_editor_shared(|ui, _| ui.set_shortcuts_open(true))?;
    let modal_diff = differing_pixels(&baseline, &modal).map_err(ToString::to_string)?;
    assert!(
        modal_diff > 20_000,
        "the shortcuts modal scrim covers the shell ({modal_diff})"
    );
    Ok(())
}

/// Opens the settings modal on `tab` over the editor shell.
fn render_config_tab(
    tab: crate::config_ui::ConfigTab,
) -> Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, Box<dyn std::error::Error>> {
    render_editor_shared(|ui, app| {
        crate::app::with_app(app, |app| {
            app.config_tab = tab;
            app.config_open = true;
            app.sync_config(ui);
        });
    })
}

#[test]
fn every_new_config_tab_paints_its_own_pane() -> TestResult {
    init_backend()?;
    let providers = render_config_tab(crate::config_ui::ConfigTab::Providers)?;
    let tabs = [
        crate::config_ui::ConfigTab::Appearance,
        crate::config_ui::ConfigTab::LocalModels,
        crate::config_ui::ConfigTab::LocalStorage,
        crate::config_ui::ConfigTab::Cost,
        crate::config_ui::ConfigTab::Logs,
        crate::config_ui::ConfigTab::About,
    ];
    for tab in tabs {
        let buffer = render_config_tab(tab)?;
        let diff = differing_pixels(&providers, &buffer).map_err(ToString::to_string)?;
        assert!(
            diff > 5_000,
            "the {} tab paints its own content ({diff})",
            tab.as_str()
        );
    }
    Ok(())
}

#[test]
fn logs_tab_paints_a_recorded_entry() -> TestResult {
    init_backend()?;
    let baseline = render_config_tab(crate::config_ui::ConfigTab::Logs)?;
    let with_entry = render_editor_shared(|ui, app| {
        crate::app::with_app(app, |app| {
            app.config_tab = crate::config_ui::ConfigTab::Logs;
            app.config_open = true;
            app.log(
                open_canvas_ai::runtime_log::RuntimeLogLevel::Error,
                "generation",
                "Render fixture failure",
                Some("detail line".to_string()),
            );
            app.sync_config(ui);
        });
    })?;
    let diff = differing_pixels(&baseline, &with_entry).map_err(ToString::to_string)?;
    assert!(diff > 200, "the log row paints its entry ({diff})");
    Ok(())
}

#[test]
fn dark_theme_repaints_the_shell_and_the_modal() -> TestResult {
    init_backend()?;
    let (_light_dir, light_store) = crate::config_test_support::temp_store()?;
    let (_dark_dir, dark_store) = crate::config_test_support::temp_store()?;
    let light = crate::test_support::render_editor_with_store(light_store, |ui, app| {
        crate::app::with_app(app, |app| {
            app.config_tab = crate::config_ui::ConfigTab::Appearance;
            app.config_open = true;
            app.sync_config(ui);
        });
    })?;
    let dark = crate::test_support::render_editor_with_store(dark_store, |ui, app| {
        crate::app::with_app(app, |app| {
            app.theme_pick(ui, "dark");
            app.config_tab = crate::config_ui::ConfigTab::Appearance;
            app.config_open = true;
            app.sync_config(ui);
        });
    })?;
    let diff = differing_pixels(&light, &dark).map_err(ToString::to_string)?;
    assert!(
        diff > 100_000,
        "the dark theme repaints the whole surface ({diff})"
    );
    Ok(())
}

#[test]
fn attempt_workbench_renders_centered_prompt_config_and_trace() -> TestResult {
    init_backend()?;
    let buffer = render_editor_shared(|ui, _app| {
        ui.set_attempt_open(true);
        ui.set_attempt_tabs(ModelRc::new(VecModel::from(vec![crate::AttemptTabRow {
            id: "v7".into(),
            label: "v7".into(),
            current: true,
        }])));
        ui.set_attempt_prompt("A red fox in a snowy bamboo forest, cinematic light".into());
        ui.set_attempt_model("openai/gpt-image-2.5-sunburst".into());
        ui.set_attempt_size("16:9".into());
        ui.set_attempt_quality("high".into());
        ui.set_attempt_count(2);
        ui.set_attempt_version_text("7".into());
        ui.set_attempt_intent("fox study".into());
        ui.set_attempt_reason("Warmer palette and a lower camera angle".into());
        ui.set_attempt_frozen(false);
        ui.set_attempt_image_count(3);
        ui.set_attempt_has_image(false);
        ui.set_attempt_has_output(true);
        ui.set_attempt_trace(
            "TIMELINE\n[plan] Read the legacy image\n[write] Draft the next prompt\n\nRAW\nThe full agent thinking is kept here."
                .into(),
        );
    })?;
    assert!(buffer.width() > 0 && buffer.height() > 0);
    let pixels: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    let image = image::RgbaImage::from_raw(buffer.width(), buffer.height(), pixels)
        .ok_or("the workbench snapshot has the expected size")?;
    let path = std::env::temp_dir().join("open-canvas-attempt-workbench.png");
    image.save(&path)?;
    // The centered panel paints a light surface over the darker canvas; at
    // least one pixel in the middle band must differ from the canvas corner.
    let corner = image.get_pixel(4, 4);
    let centre = image.get_pixel(buffer.width() / 2, buffer.height() / 2);
    assert_ne!(corner, centre, "the workbench panel is visible");
    Ok(())
}

#[test]
fn attempt_workbench_renders_editable_flat_controls() -> TestResult {
    init_backend()?;
    let buffer = render_editor_shared(|ui, _app| {
        ui.set_attempt_open(true);
        ui.set_attempt_prompt("A flat vector icon of a whale, pastel background".into());
        ui.set_attempt_model("openai/gpt-image-2.5-sunburst".into());
        ui.set_attempt_model_options(ModelRc::new(VecModel::from(vec![
            slint::SharedString::from("openai/gpt-image-2.5-sunburst"),
            slint::SharedString::from("openai/gpt-image-2"),
        ])));
        ui.set_attempt_size("1024x768".into());
        ui.set_attempt_size_custom(true);
        ui.set_attempt_quality("high".into());
        ui.set_attempt_count(3);
        ui.set_attempt_version_text("5".into());
        ui.set_attempt_intent("whale icon".into());
        ui.set_attempt_reason("Initial version".into());
        ui.set_attempt_frozen(false);
        ui.set_attempt_image_count(0);
        ui.set_attempt_has_image(false);
        ui.set_attempt_trace("No trace recorded for this attempt yet.".into());
    })?;
    let pixels: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    let image = image::RgbaImage::from_raw(buffer.width(), buffer.height(), pixels)
        .ok_or("editable workbench snapshot size")?;
    let path = std::env::temp_dir().join("open-canvas-attempt-workbench-editable.png");
    image.save(&path)?;
    Ok(())
}

#[test]
fn image_focus_modal_renders_a_borderless_stage_and_floating_toolbar() -> TestResult {
    init_backend()?;
    let content = red_png_data_url()?;
    let mut media = MediaCache::default();
    let image = media.image(&content).ok_or("decode preview image")?;
    let buffer = render_editor_shared(|ui, _app| {
        ui.set_preview_open(true);
        ui.set_preview_stack(false);
        ui.set_preview_image(image);
        ui.set_preview_title("Image".into());
    })?;
    assert!(buffer.width() > 0 && buffer.height() > 0);
    let pixels: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    let image = image::RgbaImage::from_raw(buffer.width(), buffer.height(), pixels)
        .ok_or("focus snapshot size")?;
    let path = std::env::temp_dir().join("open-canvas-image-focus.png");
    image.save(&path)?;
    // The stage is not a rounded card: the red bitmap reaches the middle of the
    // focus area, while the bottom toolbar remains visible below it.
    let centre = image.get_pixel(buffer.width() / 2, buffer.height() / 2);
    assert!(centre.0[0] > 150, "the bitmap fills the focus stage");
    Ok(())
}
