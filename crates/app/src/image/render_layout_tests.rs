//! Headless layout tests for the Image Studio chrome: the shared layout tokens,
//! the 36px layer-row geometry inside the right dock, and that a resized dock
//! reflows the rows instead of clipping them. Code-only verification (project
//! rule): the Slint testing backend embeds the software rasterizer, so
//! `Window::take_snapshot` returns a real pixel buffer.

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::ComponentHandle;

use crate::app::View;
use crate::image::dock_engine::{studio, DockEdge, STUDIO_IMAGE};
use crate::store::Store;
use crate::test_support::{differing_pixels, init_backend_allow_existing};
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Buffer = slint::SharedPixelBuffer<slint::Rgba8Pixel>;

// A tiny compiled document that only re-exports the studio palette. Slint's
// generated Rust API only exposes globals the root re-exports, and `app.slint`
// is not ours to edit, so the palette test compiles `studio-theme.slint` on its
// own and reads `StudioTheme` next to the same `Theme` instance it maps.
slint::slint! {
    import { Theme } from "../../ui/theme.slint";
    import { StudioTheme } from "../../ui/studio-theme.slint";
    export { Theme } from "../../ui/theme.slint";

    export component StudioPaletteProbe inherits Window {
        in-out property <bool> dark <=> Theme.dark;

        out property <color> menu-bar: StudioTheme.menu-bar;
        out property <color> options-bar: StudioTheme.options-bar;
        out property <color> rail: StudioTheme.rail;
        out property <color> panel-header: StudioTheme.panel-header;
        out property <color> panel-tab: StudioTheme.panel-tab;
        out property <color> panel-body: StudioTheme.panel-body;
        out property <color> status-bar: StudioTheme.status-bar;
        out property <color> menu-surface: StudioTheme.menu-surface;
        out property <color> canvas-surround: StudioTheme.canvas-surround;
        out property <color> hairline: StudioTheme.hairline;
        out property <color> separator: StudioTheme.separator;
        out property <color> text: StudioTheme.text;
        out property <color> text-strong: StudioTheme.text-strong;
        out property <color> text-muted: StudioTheme.text-muted;
        out property <color> text-faint: StudioTheme.text-faint;
        out property <color> text-disabled: StudioTheme.text-disabled;
        out property <color> selection: StudioTheme.selection;
        out property <color> selection-hover: StudioTheme.selection-hover;
        out property <color> selection-soft: StudioTheme.selection-soft;
        out property <color> hover: StudioTheme.hover;
        out property <color> control: StudioTheme.control;
        out property <color> control-border: StudioTheme.control-border;
        out property <color> control-border-strong: StudioTheme.control-border-strong;
        out property <color> field: StudioTheme.field;
        out property <color> slider-track: StudioTheme.slider-track;
        out property <color> danger: StudioTheme.danger;
        out property <color> danger-soft: StudioTheme.danger-soft;
        out property <color> studio-background: StudioTheme.background;
        out property <color> border: StudioTheme.border;
        out property <color> border-strong: StudioTheme.border-strong;
        out property <color> foreground: StudioTheme.foreground;
        out property <color> muted: StudioTheme.muted;
        out property <color> brand-foreground: StudioTheme.brand-foreground;
        out property <color> glass-panel-strong: StudioTheme.glass-panel-strong;
        out property <color> node-text: StudioTheme.node-text;
        out property <color> node-label: StudioTheme.node-label;
        out property <color> node-muted: StudioTheme.node-muted;
        out property <color> node-faint: StudioTheme.node-faint;
        out property <color> node-accent: StudioTheme.node-accent;
        out property <color> node-accent-soft: StudioTheme.node-accent-soft;
        out property <color> node-active-stroke: StudioTheme.node-active-stroke;
        out property <color> node-danger: StudioTheme.node-danger;
        out property <color> node-success: StudioTheme.node-success;
        out property <color> node-info: StudioTheme.node-info;
        out property <color> node-info-soft: StudioTheme.node-info-soft;
        out property <color> toolbar-border: StudioTheme.toolbar-border;
        out property <color> toolbar-item: StudioTheme.toolbar-item;
        out property <color> toolbar-item-hover: StudioTheme.toolbar-item-hover;
        out property <color> toolbar-active-bg: StudioTheme.toolbar-active-bg;
        out property <color> toolbar-active-text: StudioTheme.toolbar-active-text;
        out property <color> image-checker-light: StudioTheme.image-checker-light;
        out property <color> image-checker-dark: StudioTheme.image-checker-dark;
        out property <color> image-ants-dark: StudioTheme.image-ants-dark;
        out property <color> image-ants-light: StudioTheme.image-ants-light;
        out property <color> image-guide: StudioTheme.image-guide;
        out property <color> image-snap-line: StudioTheme.image-snap-line;
        out property <color> image-path-line: StudioTheme.image-path-line;
        out property <color> image-draft: StudioTheme.image-draft;
        out property <color> image-transform: StudioTheme.image-transform;
        out property <color> picker-handle: StudioTheme.picker-handle;
    }
}

/// Renders the Image Studio at a fixed window size with `n` layers and the
/// right dock at `dock` px, with the layers panel revealed (the shipped layout
/// starts with every edge empty).
fn render_image(dock: f64, n: usize) -> Result<Buffer, Box<dyn std::error::Error>> {
    render_image_appearance(dock, n, false)
}

/// `render_image` in a chosen appearance: the studio follows `Theme.dark`.
fn render_image_appearance(
    dock: f64,
    n: usize,
    dark: bool,
) -> Result<Buffer, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        // The first sync loads `dock.json` (the default layout here) and pushes
        // it, so the mutations below survive to the explicit push at the end.
        app.sync_all(&ui);
        if let Some(def) = studio(STUDIO_IMAGE) {
            app.image.dock.reveal(def, "layers");
        }
        app.image.dock.set_size(DockEdge::Right, dock);
        let mut gen = open_canvas_ps::IdGen::new("layout");
        let layers: Vec<_> = (0..n)
            .map(|i| {
                open_canvas_ps::create_ps_pixel_layer(&mut gen, (16.0, 16.0), &format!("L{i}"))
            })
            .collect();
        if let Some(layer) = layers.first() {
            app.image.active_layer = layer.id.clone();
        }
        app.image.project.board_layers = layers;
        app.sync_all(&ui);
        // `sync_all` only pushes the dock while it is dirty; push the mutated
        // layout explicitly (as `dock_render_tests` does).
        app.sync_dock(&ui, STUDIO_IMAGE);
    });
    ui.set_view("image".into());
    ui.global::<crate::Theme>().set_dark(dark);
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// The first/last row where `a` and `b` differ at or beyond column `x0`.
fn differing_y_extent(a: &Buffer, b: &Buffer, x0: u32) -> Option<(u32, u32)> {
    let width = a.width();
    let mut first = None;
    let mut last = 0;
    for y in 0..a.height() {
        let row = (y * width) as usize;
        let changed = (x0..width).any(|x| {
            let p = a.as_slice()[row + x as usize];
            let q = b.as_slice()[row + x as usize];
            p.r != q.r || p.g != q.g || p.b != q.b
        });
        if changed {
            if first.is_none() {
                first = Some(y);
            }
            last = y;
        }
    }
    first.map(|f| (f, last))
}

/// The first/last column where `a` and `b` differ within `x0..x1`.
fn differing_x_extent(a: &Buffer, b: &Buffer, x0: u32, x1: u32) -> Option<(u32, u32)> {
    let width = a.width();
    let mut first = None;
    let mut last = 0;
    for x in x0..x1 {
        let changed = (0..a.height()).any(|y| {
            let p = a.as_slice()[(y * width + x) as usize];
            let q = b.as_slice()[(y * width + x) as usize];
            p.r != q.r || p.g != q.g || p.b != q.b
        });
        if changed {
            if first.is_none() {
                first = Some(x);
            }
            last = x;
        }
    }
    first.map(|f| (f, last))
}

/// The shared layout contract (`theme.slint`): one spacing scale, one row /
/// header / control height per surface.
#[test]
fn layout_tokens_carry_the_shared_scale() -> TestResult {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let theme = ui.global::<crate::Theme>();
    assert_eq!(f64::from(theme.get_space_1()), 4.0);
    assert_eq!(f64::from(theme.get_space_2()), 8.0);
    assert_eq!(f64::from(theme.get_space_3()), 12.0);
    assert_eq!(f64::from(theme.get_space_4()), 16.0);
    assert_eq!(f64::from(theme.get_space_5()), 20.0);
    assert_eq!(f64::from(theme.get_space_6()), 24.0);
    assert_eq!(f64::from(theme.get_space_8()), 32.0);
    assert_eq!(f64::from(theme.get_panel_pad()), 12.0);
    assert_eq!(f64::from(theme.get_panel_gap()), 8.0);
    assert_eq!(f64::from(theme.get_row_h()), 28.0);
    assert_eq!(f64::from(theme.get_row_h_lg()), 32.0);
    assert_eq!(f64::from(theme.get_header_h()), 30.0);
    assert_eq!(f64::from(theme.get_control_h()), 28.0);
    assert_eq!(f64::from(theme.get_titlebar_h()), 40.0);
    Ok(())
}

/// One layer row paints exactly `row-layer-height` tall: the only difference
/// between an empty list and a one-layer list is the row itself.
#[test]
fn a_layer_row_paints_at_the_row_token() -> TestResult {
    init_backend_allow_existing();
    let empty = render_image(288.0, 0)?;
    let one = render_image(288.0, 1)?;
    let dock_x = one.width().saturating_sub(288);
    let (top, bottom) =
        differing_y_extent(&empty, &one, dock_x).ok_or("the layer row never painted")?;
    let height = bottom - top + 1;
    let row_h = f64::from(
        AppWindow::new()?
            .global::<crate::Theme>()
            .get_row_layer_height(),
    );
    assert!(
        (f64::from(height) - row_h).abs() <= 1.0,
        "the layer row paints {height}px tall, the row token is {row_h}px"
    );
    Ok(())
}

/// Resizing the right dock reflows the row instead of clipping it: the row
/// keeps its height and grows with the column width.
#[test]
fn a_resized_dock_reflows_the_layer_rows() -> TestResult {
    init_backend_allow_existing();
    let narrow_empty = render_image(200.0, 0)?;
    let narrow_one = render_image(200.0, 1)?;
    let wide_empty = render_image(420.0, 0)?;
    let wide_one = render_image(420.0, 1)?;

    let height_in = |empty: &Buffer, one: &Buffer, dock: u32| {
        differing_y_extent(empty, one, one.width().saturating_sub(dock))
            .map(|(top, bottom)| bottom - top + 1)
    };
    assert_eq!(
        height_in(&narrow_empty, &narrow_one, 200),
        Some(36),
        "the narrow dock row keeps the 36px height"
    );
    assert_eq!(
        height_in(&wide_empty, &wide_one, 420),
        Some(36),
        "the wide dock row keeps the 36px height"
    );

    let span_in = |empty: &Buffer, one: &Buffer, dock: u32| {
        differing_x_extent(empty, one, one.width().saturating_sub(dock), one.width())
            .map_or(0, |(a, b)| b - a + 1)
    };
    let narrow_span = span_in(&narrow_empty, &narrow_one, 200);
    let wide_span = span_in(&wide_empty, &wide_one, 420);
    assert!(
        wide_span > narrow_span + 100,
        "the wider dock paints a wider row ({narrow_span}px vs {wide_span}px)"
    );
    Ok(())
}

/// Renders the Image Studio with a chosen active tool (the tool rail's
/// selection is what the design-language assertion looks at).
fn render_image_tool(tool: crate::image::ImageTool) -> Result<Buffer, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.sync_all(&ui);
        app.image.tool = tool;
        app.sync_image(&ui);
    });
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// One studio chrome frame: a tool and the view state the status bar reads
/// (zoom, document size).
fn render_chrome(
    tool: crate::image::ImageTool,
    view_k: f32,
    doc_w: f32,
    doc_h: f32,
) -> Result<Buffer, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.sync_all(&ui);
        app.image.tool = tool;
        app.sync_image(&ui);
    });
    ui.set_image_view_k(view_k);
    ui.set_image_doc_w(doc_w);
    ui.set_image_doc_h(doc_h);
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok(ui.window().take_snapshot()?)
}

/// The studio palette, read at runtime from the app's `Theme` global. The
/// studio is a thin mapping over `Theme` (`studio-theme.slint`): every surface
/// is a translucent glass tier drawn over the window plate (the opaque
/// `Theme.background` when the OS backdrop is unavailable), so a snapshot pixel
/// is that tier composited over the theme's base tone.
struct Palette {
    canvas: (u8, u8, u8),
    glass_strong: (u8, u8, u8),
    glass_panel: (u8, u8, u8),
    hairline: (u8, u8, u8),
    text: (u8, u8, u8),
    muted: (u8, u8, u8),
    brand: (u8, u8, u8),
    checker_light: (u8, u8, u8),
    checker_dark: (u8, u8, u8),
}

/// `slint::Color` → opaque RGB. Alpha is dropped deliberately: the palette is
/// always compared as composited pixels.
fn rgb(color: slint::Color) -> (u8, u8, u8) {
    let argb = color.to_argb_u8();
    (argb.red, argb.green, argb.blue)
}

/// Source-over composite (sRGB bytes) as the software renderer performs it:
/// each term is premultiplied to 8 bits and truncated before the add. This is
/// what turns `rgba(244, 241, 235, 0.9)` over the same base tone into 243/240/234
/// instead of the mathematically exact 244/241/235.
fn over(top: slint::Color, bottom: (u8, u8, u8)) -> (u8, u8, u8) {
    let argb = top.to_argb_u8();
    let alpha = u32::from(argb.alpha);
    let inverse = 255 - alpha;
    let blend = |source: u8, target: u8| {
        let premultiplied = (u32::from(source) * alpha) / 255;
        let base = (u32::from(target) * inverse) / 255;
        u8::try_from((premultiplied + base).min(255)).unwrap_or(0)
    };
    (
        blend(argb.red, bottom.0),
        blend(argb.green, bottom.1),
        blend(argb.blue, bottom.2),
    )
}

/// Reads the app palette for one appearance. Panels sit over the opaque window
/// plate in the headless backend (`root.backdrop` is false there), so each glass
/// tier is composited over `Theme.background`.
fn palette(dark: bool) -> Result<Palette, Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let ui = AppWindow::new()?;
    let theme = ui.global::<crate::Theme>();
    theme.set_dark(dark);
    let background = rgb(theme.get_background());
    let canvas = rgb(theme.get_canvas_background());
    Ok(Palette {
        canvas,
        glass_strong: over(theme.get_glass_panel_strong(), background),
        glass_panel: over(theme.get_glass_panel(), background),
        hairline: over(theme.get_toolbar_border(), background),
        text: rgb(theme.get_node_text()),
        muted: rgb(theme.get_node_muted()),
        brand: rgb(theme.get_brand()),
        checker_light: over(theme.get_image_checker_light(), canvas),
        checker_dark: over(theme.get_image_checker_dark(), canvas),
    })
}

/// One pixel of a snapshot.
fn pixel_at(buffer: &Buffer, x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer.as_slice()[(y * buffer.width() + x) as usize];
    (p.r, p.g, p.b)
}

/// How many pixels in `buffer` are exactly `color`.
fn count_rgb(buffer: &Buffer, color: (u8, u8, u8)) -> usize {
    buffer
        .as_slice()
        .iter()
        .filter(|pixel| (pixel.r, pixel.g, pixel.b) == color)
        .count()
}

/// How many pixels in `buffer` sit within `tolerance` of `color` on every
/// channel (text antialiasing rarely hits the exact ink value).
fn count_near(buffer: &Buffer, color: (u8, u8, u8), tolerance: u8) -> usize {
    let near = |value: u8, target: u8| value.abs_diff(target) <= tolerance;
    buffer
        .as_slice()
        .iter()
        .filter(|pixel| near(pixel.r, color.0) && near(pixel.g, color.1) && near(pixel.b, color.2))
        .count()
}

/// True when `pixel` differs from `color` by at most `tolerance` per channel.
fn near(pixel: (u8, u8, u8), color: (u8, u8, u8), tolerance: u8) -> bool {
    let same = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    same(pixel.0, color.0) && same(pixel.1, color.1) && same(pixel.2, color.2)
}

/// The first window row that paints the studio menu bar. The studio is a
/// full-window workspace: its own 40px menu bar is the topmost row, with no app
/// title bar above it.
fn studio_top() -> u32 {
    0
}

/// The band boundaries the studio chrome lays out: the menu bar surface, its
/// closing hairline, and the board top that follows it directly (the document
/// tab strip and the 34px tool-options band are both gone). The surfaces share
/// the theme's base tone (that is what the glass tiers composite to over the
/// opaque plate), so the geometry comes from the band contract rather than from
/// a colour scan.
fn chrome_bands() -> (u32, u32, u32) {
    let top = studio_top();
    // 39px menu surface + the 1px hairline closes the 40px band.
    let menu_end = top + 39;
    // The board starts right below the hairline: no band between.
    let board_top = menu_end + 1;
    (top, menu_end, board_top)
}

/// Pixels in `x0..x1` × `y0..y1` that are not the given surface colour.
fn ink_in(buffer: &Buffer, x0: u32, x1: u32, y0: u32, y1: u32, surface: (u8, u8, u8)) -> usize {
    let mut ink = 0;
    for y in y0..y1.min(buffer.height()) {
        for x in x0..x1.min(buffer.width()) {
            if pixel_at(buffer, x, y) != surface {
                ink += 1;
            }
        }
    }
    ink
}

/// The bounding box of the pixels where two same-size snapshots differ, limited
/// to the region right of `x0` and below `y0` (the studio chrome around a
/// floating card).
fn differing_bbox(
    a: &Buffer,
    b: &Buffer,
    x0: u32,
    y0: u32,
    y1: u32,
) -> Option<(u32, u32, u32, u32)> {
    let width = a.width();
    let mut min = (u32::MAX, u32::MAX);
    let mut max = (0, 0);
    for y in y0..y1.min(a.height()) {
        for x in x0..width {
            let p = a.as_slice()[(y * width + x) as usize];
            let q = b.as_slice()[(y * width + x) as usize];
            if p.r != q.r || p.g != q.g || p.b != q.b {
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
        }
    }
    (min.0 != u32::MAX).then_some((min.0, min.1, max.0, max.1))
}

/// Where two same-size snapshots disagree vertically, within `x0..x1`.
fn differing_rows(left: &Buffer, right: &Buffer, x0: u32, x1: u32) -> Option<(u32, u32)> {
    let width = left.width();
    let mut first = None;
    let mut last = 0;
    for y in 0..left.height() {
        let changed = (x0..x1.min(width)).any(|x| {
            let a = left.as_slice()[(y * width + x) as usize];
            let b = right.as_slice()[(y * width + x) as usize];
            a.r != b.r || a.g != b.g || a.b != b.b
        });
        if changed {
            first.get_or_insert(y);
            last = y;
        }
    }
    first.map(|f| (f, last))
}

/// Design language: a studio panel's content surface is the strong glass tier
/// over the window plate — the sheet reads as a solid surface while still
/// carrying the app's frost.
#[test]
fn a_studio_panel_paints_the_glass_body_surface() -> TestResult {
    let palette = palette(false)?;
    let buffer = render_image(288.0, 0)?;
    // A vertical strip inside the right dock's panel body (past the tab strip
    // and header, left of the dock's inner edge).
    let dock_x = buffer.width().saturating_sub(288) + 40;
    let mut painted = 0;
    for y in 160..640 {
        if near(pixel_at(&buffer, dock_x, y), palette.glass_strong, 1) {
            painted += 1;
        }
    }
    assert!(
        painted > 300,
        "the panel body paints the strong glass tier ({painted} px)"
    );
    Ok(())
}

/// Design language: the tool rail's active tool is a solid selection block, not
/// a pale tint, and it follows the selection.
#[test]
fn the_tool_rail_active_tool_paints_a_solid_selection_block() -> TestResult {
    let palette = palette(false)?;
    let hand_active = render_image_tool(crate::image::ImageTool::Hand)?;
    let pen_active = render_image_tool(crate::image::ImageTool::Pen)?;
    let hand_block = count_rgb(&hand_active, palette.brand);
    let pen_block = count_rgb(&pen_active, palette.brand);
    assert!(
        hand_block > 200,
        "the active rail button paints a solid selection block ({hand_block} px)"
    );
    assert!(
        pen_block > 200,
        "the other selection paints its own block ({pen_block} px)"
    );
    assert!(
        differing_pixels(&hand_active, &pen_active)? > 100,
        "switching the active tool moves the block"
    );
    Ok(())
}

/// The File menu's PSD entry routes to its path prompt (Rust-level wiring:
/// `file.psd-export` is the id the menu row sends, and the prompt opens
/// pre-filled with a `.psd` path next to the export folder).
#[test]
fn file_menu_routes_psd_export_to_its_path_prompt() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.image.meta = Some(crate::image::ImageMeta {
            id: "img-1".to_string(),
            title: "Board".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            group_id: None,
        });

        app.image_action(&ui, "file.psd-export");
        assert!(
            app.image
                .psd_export
                .as_ref()
                .is_some_and(|flow| flow.path.ends_with("Board.psd") && !flow.error),
            "Export as PSD opens its path prompt pre-filled beside the exports"
        );
    });
    Ok(())
}

/// File is export-only: exactly the four composite exports, each sending its
/// own action, and every removed entry is gone. The raster entries share one
/// prompt, pre-filled with the format's extension in the project's export
/// folder.
#[test]
fn file_menu_is_export_only_and_each_entry_opens_its_own_format() -> TestResult {
    init_backend_allow_existing();
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    let file_menu = &src[src
        .find("if root.menu == \"file\" : StudioMenu {")
        .ok_or("the File menu is gone")?..];
    let file_menu = &file_menu[..file_menu
        .find("if root.menu == \"layer\"")
        .ok_or("the File menu has no end")?];
    for (label, action) in [
        ("Export as PNG...", "file.png"),
        ("Export as JPEG...", "file.jpeg"),
        ("Export as BMP...", "file.bmp"),
        ("Export as PSD...", "file.psd-export"),
    ] {
        assert!(
            file_menu.contains(&format!("label: \"{label}\";")),
            "File keeps {label}"
        );
        assert!(
            file_menu.contains(&format!(r#"root.action("{action}")"#)),
            "{label} sends {action}"
        );
    }
    for removed in [
        "Import Bitmap...",
        "Open PSD...",
        "Export...",
        "Save to Canvas",
        "Close Image",
        "output.open",
        "file.import",
        "file.psd-import",
        "file.save-canvas",
    ] {
        assert!(
            !file_menu.contains(removed),
            "the File menu no longer offers {removed}"
        );
    }

    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.image.meta = Some(crate::image::ImageMeta {
            id: "img-1".to_string(),
            title: "Board".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            group_id: None,
        });
        for (action, extension) in [
            ("file.png", "png"),
            ("file.jpeg", "jpg"),
            ("file.bmp", "bmp"),
        ] {
            app.image.export = None;
            app.image_action(&ui, action);
            let Some(flow) = app.image.export.clone() else {
                return;
            };
            assert!(
                std::path::Path::new(&flow.path)
                    .extension()
                    .is_some_and(|found| found.eq_ignore_ascii_case(extension)),
                "{action} pre-fills a .{extension} path ({})",
                flow.path
            );
            assert!(!flow.error);
        }
        // The removed ids no longer open anything.
        app.image.menu.clear();
        app.image.export = None;
        app.image.import = None;
        for removed in [
            "file.import",
            "file.psd-import",
            "file.save-canvas",
            "output.open",
        ] {
            app.image_action(&ui, removed);
            assert!(
                app.image.export.is_none() && app.image.import.is_none(),
                "{removed} is no longer wired"
            );
        }
    });
    Ok(())
}

/// The Filter menu is three nested flyouts, not one long scrolling list: the
/// top level opens a group flyout, Filter Gallery's flyout opens the six
/// gallery folders, and the flyout position flips left near the right edge.
#[test]
fn the_filter_menu_opens_nested_flyouts() {
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    assert!(
        !src.contains("StudioScrollMenu"),
        "the scrolling filter list is gone"
    );
    assert!(
        src.contains("if root.menu == \"filter\" && root.filter-open-group != \"\""),
        "the first flyout is a conditional popup"
    );
    assert!(
        src.contains("if root.menu == \"filter\" && root.filter-open-sub != \"\""),
        "the gallery folders open a third level"
    );
    assert!(
        src.contains("pure function filter-flyout-x"),
        "flyouts are placed against the window edge"
    );
    assert!(
        src.contains("e.text == Key.Escape && root.menu != \"\""),
        "Escape closes the menu"
    );
}

/// The Filter popup is a compact nested menu, not the old 540px scrolling
/// column: it paints directly under Filter, hovering a group row opens a
/// flyout to its right, and both stay inside the window.
#[test]
#[expect(
    clippy::cast_precision_loss,
    reason = "snapshot pixel coordinates are far below f32's exact range"
)]
fn the_filter_menu_paints_a_compact_popup_and_a_right_flyout() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.sync_all(&ui);
    });
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    // The closed studio is the baseline: only the popup counts as a change.
    let closed = ui.window().take_snapshot()?;

    crate::app::with_app(&app, |app| {
        app.image.menu = "filter".to_string();
        app.sync_image(&ui);
    });
    ui.window().request_redraw();
    let open = ui.window().take_snapshot()?;

    // The Filter tab itself reacts too, so measure only below the menu bar.
    let (x0, y0, x1, y1) =
        differing_bbox(&closed, &open, 0, 40, 800).ok_or("the Filter menu never painted")?;
    assert!(y0 >= 40, "the popup starts below the menu bar ({y0})");
    assert!(
        y1 - y0 < 380,
        "the popup hugs its eleven rows instead of scrolling ({y0}..{y1})"
    );
    assert!(x0 > 200, "the popup hangs under the Filter tab ({x0})");
    assert!(x1 < 640, "the popup is one column wide ({x0}..{x1})");

    // Rows: 1px border + 4px padding, then Repeat (28), a 5px divider, then the
    // groups. Hover the first group row (Blur) and its flyout opens right of
    // the popup, its top lined up with the row.
    let blur_row = (y0 + 52) as f32;
    let position = slint::LogicalPosition::new((x0 + 20) as f32, blur_row);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    ui.window().request_redraw();
    let flyout = ui.window().take_snapshot()?;
    assert_eq!(
        ui.get_image_menu().as_str(),
        "filter",
        "hovering a group row keeps the menu open"
    );
    let (fx0, fy0, fx1, fy1) = differing_bbox(&open, &flyout, x1 + 1, 40, 800)
        .ok_or("hovering a group row opens its flyout to the right")?;
    assert!(fx0 >= x1, "the flyout starts past the first popup ({fx0})");
    assert!(fx1 < 1280, "the flyout stays inside the window ({fx1})");
    assert!(
        fx1 - fx0 < 300,
        "the flyout is one column wide ({fx0}..{fx1})"
    );
    assert!(
        fy0 >= 40 && fy1 < 800,
        "the flyout is clamped vertically ({fy0}..{fy1})"
    );

    // Filter Gallery is the last group row; its flyout holds the six folders,
    // and the first folder opens the third level.
    let gallery = (y0 + 38 + 9 * 28 + 14) as f32;
    let position = slint::LogicalPosition::new((x0 + 20) as f32, gallery);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    ui.window().request_redraw();
    let gallery_flyout = ui.window().take_snapshot()?;
    let (gx0, gy0, _, _) = differing_bbox(&open, &gallery_flyout, x1 + 1, 40, 800)
        .ok_or("the Filter Gallery row opens its folder list")?;
    let folder = (gy0 + 19) as f32;
    let position = slint::LogicalPosition::new((gx0 + 20) as f32, folder);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    ui.window().request_redraw();
    let third = ui.window().take_snapshot()?;
    let (tx0, ty0, tx1, ty1) = differing_bbox(&gallery_flyout, &third, gx0 + 200, 40, 800)
        .ok_or("a gallery folder opens its own flyout")?;
    assert!(tx0 > gx0, "the folder flyout is the third level ({tx0})");
    assert!(
        tx1 < 1280,
        "the third level stays inside the window ({tx1})"
    );
    assert!(
        ty0 >= 40 && ty1 < 800,
        "the third level is clamped ({ty0}..{ty1})"
    );

    // Escape closes the menu and every flyout.
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::SharedString::from("\u{1b}"),
    });
    assert_eq!(ui.get_image_menu().as_str(), "", "Escape closes the menu");

    // Clicking away from the popups closes it too.
    crate::app::with_app(&app, |app| {
        app.image.menu = "filter".to_string();
        app.sync_image(&ui);
    });
    let away = slint::LogicalPosition::new(900.0, 600.0);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: away });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: away,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: away,
        button: PointerEventButton::Left,
    });
    assert_eq!(
        ui.get_image_menu().as_str(),
        "",
        "a click outside the popups closes the menu"
    );
    Ok(())
}

/// The status bar reads the view state: the zoom % and the document size.
#[test]
fn the_status_bar_carries_zoom_and_dimensions() -> TestResult {
    init_backend_allow_existing();
    let base = render_chrome(crate::image::ImageTool::Hand, 1.0, 640.0, 360.0)?;
    let zoomed = render_chrome(crate::image::ImageTool::Hand, 0.25, 640.0, 360.0)?;
    let resized = render_chrome(crate::image::ImageTool::Hand, 1.0, 320.0, 180.0)?;

    let status_top = base.height().saturating_sub(24);

    let (z_first, z_last) =
        differing_rows(&base, &zoomed, 0, 220).ok_or("the zoom % never painted")?;
    assert!(
        z_first >= status_top && z_last < base.height(),
        "the zoom read-out lives in the status bar ({z_first}..{z_last}, bar starts at {status_top})"
    );

    let (d_first, d_last) =
        differing_rows(&base, &resized, 220, 520).ok_or("the dimensions never painted")?;
    assert!(
        d_first >= status_top && d_last < base.height(),
        "the document size lives in the status bar ({d_first}..{d_last})"
    );
    Ok(())
}

/// The board shows a normal, centred document over the theme's canvas tone; the
/// transparency checker sits only under the document, is small (8px) and is a
/// low-contrast pair of warm neutrals, not the pale chessboard of old.
#[test]
fn the_checkerboard_is_small_low_contrast_and_inside_the_document() -> TestResult {
    let palette = palette(false)?;
    let frame = render_transparent_board(200.0, 120.0)?;
    let (dx, dy, dw, dh) = frame.doc;
    let (bx, by, bw, bh) = frame.board;

    // Centred: equal margins left/right and top/bottom (within a pixel).
    let left = dx - bx;
    let right = bx + bw - (dx + dw);
    let top = dy - by;
    let bottom = by + bh - (dy + dh);
    assert!(
        (left - right).abs() <= 1.0 && (top - bottom).abs() <= 1.0,
        "the document is centred (margins {left}/{right} × {top}/{bottom})"
    );
    assert!(dw > 0.0 && dh > 0.0, "the document has a positive size");

    // The two checker tones: sample the centres of the first two 8px tiles
    // (the painter anchors the pattern to the document's top-left corner).
    let sample = |x: f64, y: f64| {
        let px = u32::try_from(crate::vm::f2i(x).max(0)).unwrap_or(0);
        let py = u32::try_from(crate::vm::f2i(y).max(0)).unwrap_or(0);
        pixel_at(&frame.buffer, px, py)
    };
    let light = sample(dx + 4.0, dy + 4.0);
    let dark = sample(dx + 12.0, dy + 4.0);
    assert!(
        near(light, palette.checker_light, 2),
        "the light tile composites to the theme's light checker tone ({light:?})"
    );
    assert!(
        near(dark, palette.checker_dark, 2),
        "the dark tile composites to the theme's dark checker tone ({dark:?})"
    );
    let delta = (
        light.0.abs_diff(dark.0),
        light.1.abs_diff(dark.1),
        light.2.abs_diff(dark.2),
    );
    assert!(
        delta.0 <= 14 && delta.1 <= 14 && delta.2 <= 14,
        "the checker is low contrast ({delta:?})"
    );
    assert!(
        delta.0.max(delta.1).max(delta.2) >= 4,
        "the two tiles are still distinguishable ({delta:?})"
    );

    // Nothing but the document rect paints the checker: the board outside the
    // document (ignoring a 1px antialiasing ring at the document edge) is
    // exactly the canvas surround.
    let x0 = u32::try_from(crate::vm::f2i(bx)).unwrap_or(0);
    let y0 = u32::try_from(crate::vm::f2i(by)).unwrap_or(0);
    let x1 = u32::try_from(crate::vm::f2i(bx + bw)).unwrap_or(0);
    let y1 = u32::try_from(crate::vm::f2i(by + bh)).unwrap_or(0);
    let inner_left = u32::try_from(crate::vm::f2i(dx) - 1).unwrap_or(0);
    let inner_top = u32::try_from(crate::vm::f2i(dy) - 1).unwrap_or(0);
    let inner_right = u32::try_from(crate::vm::f2i(dx + dw) + 1).unwrap_or(0);
    let inner_bottom = u32::try_from(crate::vm::f2i(dy + dh) + 1).unwrap_or(0);
    let mut outside = 0;
    for y in y0..y1.min(frame.buffer.height()) {
        for x in x0..x1.min(frame.buffer.width()) {
            if x >= inner_left && x <= inner_right && y >= inner_top && y <= inner_bottom {
                continue;
            }
            if pixel_at(&frame.buffer, x, y) != palette.canvas {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "the checker never paints the surround ({outside} px outside the document)"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// The Photoshop chrome: one band per assertion, read from the studio palette.
// ---------------------------------------------------------------------------

/// The palette itself: every studio band paints the app's glass tiers over the
/// window plate, in light and in dark. The expected pixels are read from the
/// live `Theme` global, so the studio and the rest of the window can never drift
/// apart again.
#[test]
fn the_studio_chrome_paints_the_app_glass_palette() -> TestResult {
    init_backend_allow_existing();
    for dark in [false, true] {
        let palette = palette(dark)?;
        let buffer = render_image_appearance(288.0, 1, dark)?;
        let (top, menu_end, board_top) = chrome_bands();
        let width = buffer.width();
        let height = buffer.height();
        let mode = if dark { "dark" } else { "light" };

        // 39px menu bar surface plus its 1px hairline (the 40px band).
        assert!(
            near(pixel_at(&buffer, 600, top + 2), palette.glass_strong, 1),
            "the menu bar paints the strong glass tier ({mode})"
        );
        assert!(
            near(pixel_at(&buffer, 600, menu_end), palette.hairline, 2),
            "the menu bar closes on the theme's hairline ({mode})"
        );
        assert_eq!(menu_end - top + 1, 40, "the menu bar is 40px tall");
        // The board follows the hairline directly: the document tab strip and
        // the options band are gone, so the canvas starts right under the menu.
        assert_eq!(
            board_top,
            menu_end + 1,
            "no band between the menu hairline and the board ({mode})"
        );
        assert_eq!(board_top, top + 40, "the studio chrome is the menu bar");
        assert_eq!(
            pixel_at(&buffer, 760, board_top + 2),
            palette.canvas,
            "the canvas starts directly under the menu bar ({mode})"
        );
        // The tool rail is the strong glass tier on its canvas edge.
        assert!(
            near(
                pixel_at(&buffer, 4, board_top + 80),
                palette.glass_strong,
                1
            ),
            "the tool rail paints the strong glass tier ({mode})"
        );
        // The panel group's tab strip, active tab and body in the right dock.
        let dock_x = width - 288 + 40;
        assert!(
            near(
                pixel_at(&buffer, 1200, board_top + 2),
                palette.glass_panel,
                1
            ),
            "the panel header paints the softer glass tier ({mode})"
        );
        assert!(
            near(
                pixel_at(&buffer, dock_x, board_top + 2),
                palette.glass_strong,
                1
            ),
            "the active panel tab paints the strong glass tier ({mode})"
        );
        assert!(
            near(
                pixel_at(&buffer, dock_x, board_top + 200),
                palette.glass_strong,
                1
            ),
            "the panel body paints the strong glass tier ({mode})"
        );
        // The canvas surround is the theme's canvas tone, right of the document.
        assert_eq!(
            pixel_at(&buffer, 950, board_top + 400),
            palette.canvas,
            "the canvas surround paints the canvas background ({mode})"
        );
        // The status bar is the strong glass tier under a hairline.
        assert!(
            near(pixel_at(&buffer, 600, height - 3), palette.glass_strong, 1),
            "the status bar paints the strong glass tier ({mode})"
        );
        assert!(
            near(pixel_at(&buffer, 600, height - 24), palette.hairline, 2),
            "the status bar is separated by the theme's hairline ({mode})"
        );
        // Text ink and the selection accent really are on screen.
        assert!(
            count_near(&buffer, palette.text, 2) > 100,
            "the chrome ink paints the theme's node text ({mode})"
        );
        assert!(
            count_near(&buffer, palette.muted, 2) > 100,
            "muted ink paints the theme's muted node ink ({mode})"
        );
        assert!(
            count_rgb(&buffer, palette.brand) > 200,
            "the active tool paints the theme's brand accent ({mode})"
        );
    }
    Ok(())
}

/// The palette mapping itself, at runtime: every colour `studio-theme.slint`
/// exports equals the matching `Theme` token, in light and after a dark switch.
/// This is the studio-follows-the-app contract in its strongest form — no
/// colour is a studio literal, so the studio cannot hold a look of its own.
#[test]
fn the_studio_palette_maps_the_theme_tokens() -> TestResult {
    init_backend_allow_existing();
    let probe = StudioPaletteProbe::new()?;
    macro_rules! same {
        ($studio:ident, $theme:ident) => {
            assert_eq!(
                probe.$studio(),
                probe.global::<Theme>().$theme(),
                concat!(
                    "StudioTheme.",
                    stringify!($studio),
                    " must map Theme.",
                    stringify!($theme)
                )
            );
        };
    }
    for dark in [false, true] {
        probe.global::<Theme>().set_dark(dark);
        // Chrome surfaces: the strong glass tier, with the panel header one step
        // softer.
        same!(get_menu_bar, get_glass_panel_strong);
        same!(get_options_bar, get_glass_panel_strong);
        same!(get_rail, get_glass_panel_strong);
        same!(get_status_bar, get_glass_panel_strong);
        same!(get_menu_surface, get_glass_panel_strong);
        same!(get_panel_tab, get_glass_panel_strong);
        same!(get_panel_body, get_glass_panel_strong);
        same!(get_panel_header, get_glass_panel);
        same!(get_canvas_surround, get_canvas_background);
        same!(get_hairline, get_toolbar_border);
        same!(get_separator, get_toolbar_border);
        // Ink.
        same!(get_text, get_node_text);
        same!(get_text_strong, get_node_text);
        same!(get_text_muted, get_node_muted);
        same!(get_text_faint, get_node_muted);
        same!(get_text_disabled, get_node_faint);
        // Interaction states: the brand accent and its wash.
        same!(get_selection, get_brand);
        same!(get_selection_hover, get_brand);
        same!(get_selection_soft, get_node_accent_soft);
        same!(get_hover, get_hover);
        same!(get_control, get_card);
        same!(get_control_border, get_toolbar_border);
        same!(get_control_border_strong, get_border_strong);
        same!(get_field, get_muted);
        same!(get_slider_track, get_border);
        same!(get_danger, get_node_danger);
        same!(get_danger_soft, get_node_danger_soft);
        // The studio's own token names carry the same roles.
        same!(get_studio_background, get_background);
        same!(get_border, get_toolbar_border);
        same!(get_border_strong, get_border_strong);
        same!(get_foreground, get_foreground);
        same!(get_muted, get_muted);
        same!(get_brand_foreground, get_brand_foreground);
        same!(get_glass_panel_strong, get_glass_panel_strong);
        same!(get_node_text, get_node_text);
        same!(get_node_label, get_node_label);
        same!(get_node_muted, get_node_muted);
        same!(get_node_faint, get_node_muted);
        same!(get_node_accent, get_brand);
        same!(get_node_accent_soft, get_node_accent_soft);
        same!(get_node_active_stroke, get_node_active_stroke);
        same!(get_node_danger, get_node_danger);
        same!(get_node_success, get_node_success);
        same!(get_node_info, get_node_info);
        same!(get_node_info_soft, get_node_info_soft);
        same!(get_toolbar_border, get_toolbar_border);
        same!(get_toolbar_item, get_toolbar_item);
        same!(get_toolbar_item_hover, get_toolbar_item_hover);
        same!(get_toolbar_active_bg, get_toolbar_active_bg);
        same!(get_toolbar_active_text, get_toolbar_active_text);
        // Canvas-surface values are shared, not theme-driven.
        same!(get_image_checker_light, get_image_checker_light);
        same!(get_image_checker_dark, get_image_checker_dark);
        same!(get_image_ants_dark, get_image_ants_dark);
        same!(get_image_ants_light, get_image_ants_light);
        same!(get_image_guide, get_image_guide);
        same!(get_image_snap_line, get_image_snap_line);
        same!(get_image_path_line, get_image_path_line);
        same!(get_image_draft, get_image_draft);
        same!(get_image_transform, get_image_transform);
        same!(get_picker_handle, get_picker_handle);
    }

    // The switch really moves the mapped palette (a binding that silently read
    // stale values would pass everything above).
    probe.global::<Theme>().set_dark(false);
    let light = (probe.get_studio_background(), probe.get_menu_bar());
    probe.global::<Theme>().set_dark(true);
    let dark = (probe.get_studio_background(), probe.get_menu_bar());
    assert_ne!(light, dark, "the studio palette follows Theme.dark");
    Ok(())
}

/// The options band and the document tab strip are gone: the canvas owns the
/// rows they used to take, and the board starts directly under the 40px menu
/// bar.
#[test]
fn the_options_bar_band_is_gone_and_the_canvas_moved_up() -> TestResult {
    let palette = palette(false)?;
    let buffer = render_chrome(crate::image::ImageTool::Hand, 1.0, 640.0, 360.0)?;
    let (top, menu_end, board_top) = chrome_bands();

    // The whole source of the band is gone: no marker, no element that sizes
    // itself to the band token, and the options now ride in the floating card.
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    assert!(
        !src.contains("// ---- options bar"),
        "the options bar band is deleted from the studio layout"
    );
    assert!(
        !src.contains("options-bar-h"),
        "no studio element claims the 34px band height any more"
    );
    assert!(
        src.contains("tool-options-shown") && src.contains("ImageToolOptions"),
        "the tool options live in the floating card"
    );

    // The board begins where the menu bar ends: the tab strip and the options
    // band are both gone, so no band paints between them.
    assert_eq!(
        board_top,
        menu_end + 1,
        "no band between the hairline and the board"
    );
    assert_eq!(
        board_top,
        top + 40,
        "the board starts under the 40px menu bar"
    );
    assert!(
        near(pixel_at(&buffer, 600, board_top + 2), palette.canvas, 1),
        "the canvas sits right under the menu bar (rulers off by default)"
    );
    Ok(())
}

/// The floating tool-options card: anchored 12px right of the rail, flush under
/// the menu bar, and only on screen for a tool that owns settings.
#[test]
fn the_tool_options_card_floats_beside_the_rail() -> TestResult {
    init_backend_allow_existing();
    let palette = palette(false)?;
    let brush = render_chrome(crate::image::ImageTool::Brush, 1.0, 640.0, 360.0)?;
    let plain = render_chrome(crate::image::ImageTool::Pen, 1.0, 640.0, 360.0)?;
    let hand = render_chrome(crate::image::ImageTool::Hand, 1.0, 640.0, 360.0)?;
    let (top, _, _) = chrome_bands();

    // Two tools without options (Pen, Hand) differ only in the rail button:
    // nothing to the right of the 40px rail changes.
    assert!(
        differing_bbox(&plain, &hand, 44, 0, brush.height() - 24).is_none(),
        "a tool without options paints no card"
    );

    // Brush does: the card appears right of the rail, in one compact column.
    let (x0, y0, x1, y1) = differing_bbox(&brush, &hand, 44, 0, brush.height() - 24)
        .ok_or("the brush card never painted")?;
    let rail = 40.0;
    assert!(
        (rail..=(rail + 14.0)).contains(&f64::from(x0)),
        "the card starts a 12px gap right of the rail ({x0})"
    );
    assert!(
        (228.0..=262.0).contains(&f64::from(x1 - x0)),
        "the card is a ~236px column ({})",
        x1 - x0
    );
    let card_top = top + 40;
    assert!(
        y0 <= card_top + 12 && (card_top..=(card_top + 4)).contains(&y0),
        "the card is top-aligned under the menu bar ({y0} vs {card_top})"
    );
    assert!(
        y1 > card_top + 120 && y1 < card_top + 360,
        "the card is auto-height: tall enough for its controls, not a band ({y1})"
    );
    assert!(x1 < 640, "the card never spans the board ({x1} < 640)");

    // The header paints the tool's glyph and name inside the card's top row.
    let header_surface = pixel_at(&brush, x0 + 140, y0 + 8);
    let header_ink = ink_in(&brush, x0 + 4, x1 - 4, y0 + 2, y0 + 20, header_surface);
    assert!(
        header_ink > 20,
        "the card's header paints the brush glyph and its name ({header_ink} px)"
    );

    // The card really carries the brush controls: the three slider fills paint
    // in the studio's selection accent, which the empty state does not.
    let brand = count_rgb(&brush, palette.brand);
    let baseline = count_rgb(&hand, palette.brand);
    assert!(
        brand > baseline + 180,
        "the card's sliders paint their fills ({brand} vs {baseline} brand px)"
    );
    Ok(())
}

/// The card's close affordance hides it, and picking a tool again brings it
/// back: the dismissal is never a dead end.
#[test]
fn the_card_closes_from_its_own_affordance() -> TestResult {
    init_backend_allow_existing();
    let palette = palette(false)?;
    let (top, _, _) = chrome_bands();

    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.sync_all(&ui);
        app.image.tool = crate::image::ImageTool::Brush;
        app.sync_image(&ui);
    });
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    let open = ui.window().take_snapshot()?;
    let open_brand = count_rgb(&open, palette.brand);

    // The close × sits in the card's header, top-right: rail (40) + gap (12) +
    // card (236) - padding (10) - button (16) = x 262..278, one padding row
    // under the 40px menu bar.
    let card_x = 52.0f32;
    let card_y = f32::from(u16::try_from(top + 40).unwrap_or(80));
    let (x, y) = (card_x + 236.0 - 10.0 - 8.0, card_y + 10.0 + 8.0);
    let window = ui.window();
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    ui.window().request_redraw();
    let closed = ui.window().take_snapshot()?;
    assert!(
        differing_pixels(&open, &closed)? > 1000,
        "the close affordance removes the card"
    );
    assert!(
        count_rgb(&closed, palette.brand) + 120 < open_brand,
        "the card's slider fills are gone after the close ({} vs {open_brand})",
        count_rgb(&closed, palette.brand)
    );

    // Picking another tool that owns settings reopens it: the dismissal belongs
    // to the tool it was made on, not to the card.
    crate::app::with_app(&app, |app| app.image_tool(&ui, "eraser"));
    assert_eq!(ui.get_image_active_tool().as_str(), "eraser");
    ui.window().request_redraw();
    let reopened = ui.window().take_snapshot()?;
    assert!(
        count_rgb(&reopened, palette.brand) + 120 > open_brand,
        "selecting the eraser reopens the card ({} vs {open_brand})",
        count_rgb(&reopened, palette.brand)
    );

    // A tool without options still leaves the canvas clean.
    crate::app::with_app(&app, |app| app.image_tool(&ui, "pen"));
    ui.window().request_redraw();
    let moved = ui.window().take_snapshot()?;
    assert!(
        differing_bbox(&moved, &closed, 44, 0, moved.height() - 24).is_none(),
        "the card stays closed for a tool without options"
    );
    Ok(())
}

/// Every control the band used to carry still reaches the same studio setting:
/// the card is a re-layout, not a new model. The source keeps every key, and the
/// Rust setters behind them still move the state.
#[test]
fn the_card_controls_keep_mutating_their_settings() -> TestResult {
    let src = include_str!("../../ui/components/image-panels.slint").replace("\r\n", "\n");
    // Sliders: paint, tolerance, selection, gradient, shape stroke.
    for key in [
        "brush",
        "hardness",
        "opacity",
        "tolerance",
        "feather",
        "gradient-opacity",
        "gradient-stop",
        "stroke-width",
    ] {
        assert!(
            src.contains(&format!(r#"root.slider("{key}", value)"#)),
            "the card keeps the `{key}` slider"
        );
    }
    // Toggles.
    for id in [
        "contiguous",
        "sample-all",
        "antialias",
        "gradient-radial",
        "gradient-reverse",
        "shape-fill",
    ] {
        assert!(
            src.contains(&format!(r#"root.toggle("{id}")"#)),
            "the card keeps the `{id}` toggle"
        );
    }
    // Shape chips, transform modes and the fg/bg pair.
    for kind in [
        "rectangle",
        "rounded-rectangle",
        "ellipse",
        "polygon",
        "line",
    ] {
        assert!(
            src.contains(&format!(r#"root.shape-pick("{kind}")"#)),
            "the card keeps the `{kind}` shape chip"
        );
    }
    for mode in ["free", "skew", "distort", "perspective", "warp"] {
        assert!(
            src.contains(&format!(r#"root.transform-pick("{mode}")"#)),
            "the card keeps the `{mode}` transform chip"
        );
    }
    assert!(src.contains(r#"root.color-slot("fg")"#) && src.contains(r#"root.color-slot("bg")"#));
    assert!(src.contains("root.color-swap()") && src.contains("root.color-reset()"));

    // And the settings those controls send still mutate the studio state.
    let dir = tempfile::TempDir::new()?;
    let app = Rc::new(RefCell::new(crate::app::App::new(Store::at(
        dir.path().to_path_buf(),
    ))));
    crate::app::with_app(&app, |app| {
        assert!(app.image_slider_core("brush", 40.0));
        assert_eq!(app.image.brush_size, 40.0, "Size still sets the brush size");
        assert!(app.image_slider_core("hardness", 25.0));
        assert!(
            (app.image.brush_hardness - 0.25).abs() < 1e-9,
            "Hardness still sets the brush hardness"
        );
        assert!(app.image_slider_core("opacity", 60.0));
        assert!(
            (app.image.brush_opacity - 0.60).abs() < 1e-9,
            "Opacity still sets the brush opacity"
        );
        assert!(app.image_slider_core("tolerance", 90.0));
        assert_eq!(
            app.image.tolerance, 90.0,
            "Tolerance still sets the wand tolerance"
        );
        assert!(app.image_slider_core("feather", 12.0));
        assert_eq!(
            app.image.feather, 12.0,
            "Feather still sets the selection feather"
        );
        assert!(app.image_slider_core("stroke-width", 7.0));
        assert_eq!(
            app.image.shape_stroke_width, 7.0,
            "Stroke sets the shape stroke"
        );
        assert!(app.image_slider_core("gradient-opacity", 30.0));
        assert_eq!(
            app.image.gradient_opacity, 30.0,
            "Opacity still sets the gradient opacity"
        );
        assert!(app.image_slider_core("gradient-stop", 45.0));
        assert!(
            (app.image.gradient_stop - 0.45).abs() < 1e-9,
            "Mid Stop still sets the gradient stop"
        );
        assert!(
            app.image_gradient_blend_core("multiply"),
            "Blend still sets the gradient blend"
        );
        assert_eq!(app.image.gradient_blend, "multiply");
    });
    Ok(())
}

/// One headless board frame: a transparent document at a pinned view transform,
/// with the rulers and the grid off so only the surround, the checker and the
/// empty board paint. `doc` and `board` are window-logical rectangles
/// `(x, y, w, h)`.
struct BoardFrame {
    buffer: Buffer,
    doc: (f64, f64, f64, f64),
    board: (f64, f64, f64, f64),
}

/// Headless studio board around a document of `width` × `height` document
/// pixels. `view` pins the transform (`None` fits and centres the document, as
/// the studio does on open); `layers` installs a pixel layer on the board (an
/// empty bitmaps map makes it transparent, so the checker shows through).
fn render_board(
    width: f64,
    height: f64,
    view: Option<crate::image::ImageView>,
    layers: bool,
) -> Result<BoardFrame, Box<dyn std::error::Error>> {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.image.meta = Some(crate::image::ImageMeta {
            id: "checker".to_string(),
            title: "Checker".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            group_id: None,
        });
        app.image.project.width = width;
        app.image.project.height = height;
        app.image.project.board_background = "transparent".to_string();
        app.image.project.board_layers.clear();
        if layers {
            let mut gen = open_canvas_ps::IdGen::new("board");
            let mut layer =
                open_canvas_ps::create_ps_pixel_layer(&mut gen, (width, height), "Background");
            layer.storage_key = Some("board:1".to_string());
            app.image.active_layer = layer.id.clone();
            app.image.project.board_layers = vec![layer];
        }
        app.image.show_rulers = false;
        app.image.show_grid = false;
    });
    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;

    // The board-area size is only known after the first layout pass: pin the
    // view (or fit the document to that exact rect), then read it back.
    let canvas = (
        f64::from(ui.get_image_canvas_width()),
        f64::from(ui.get_image_canvas_height()),
    );
    let mut fitted = crate::image::ImageView::default();
    crate::app::with_app(&app, |app| {
        app.image.canvas = canvas;
        if let Some(view) = view {
            // `sync_image` auto-fits once per project+size key; pin the key so
            // the pinned view survives every later sync.
            app.image.fitted = format!("checker:{width}x{height}");
            app.image.view = view;
        }
        app.sync_all(&ui);
        fitted = app.image.view;
    });
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;

    // The board is the last stretch of the studio column: right of the 40px
    // rail and above the 24px status bar.
    let board = (
        1280.0 - canvas.0,
        800.0 - 24.0 - canvas.1,
        canvas.0,
        canvas.1,
    );
    let doc = (
        board.0 + fitted.x,
        board.1 + fitted.y,
        width * fitted.k,
        height * fitted.k,
    );
    Ok(BoardFrame { buffer, doc, board })
}

/// Renders the Image Studio board around an empty (fully transparent) document
/// of `width` × `height` document pixels, fitted and centred as on open.
fn render_transparent_board(
    width: f64,
    height: f64,
) -> Result<BoardFrame, Box<dyn std::error::Error>> {
    render_board(width, height, None, false)
}

/// The checker frame is the document rect by construction: the studio binds it
/// to the view-model geometry, which is the view offset and the scaled document
/// size — never a plate of its own.
#[test]
fn the_checker_frame_is_the_document_rect() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.image.project.width = 200.0;
        app.image.project.height = 120.0;
        app.image.canvas = (800.0, 600.0);
        app.image.fitted = "checker:200x120".to_string();
        app.image.view = crate::image::ImageView {
            x: 137.0,
            y: 63.0,
            k: 0.5,
        };
        app.sync_image(&ui);
    });

    assert_eq!(
        ui.get_image_checker_x(),
        ui.get_image_view_x(),
        "checker-x is the view x"
    );
    assert_eq!(
        ui.get_image_checker_y(),
        ui.get_image_view_y(),
        "checker-y is the view y"
    );
    assert!(
        (f64::from(ui.get_image_checker_w())
            - f64::from(ui.get_image_doc_w()) * f64::from(ui.get_image_view_k()))
        .abs()
            < 1e-6,
        "checker-w is the scaled document width"
    );
    assert!(
        (f64::from(ui.get_image_checker_h())
            - f64::from(ui.get_image_doc_h()) * f64::from(ui.get_image_view_k()))
        .abs()
            < 1e-6,
        "checker-h is the scaled document height"
    );

    // The board binds exactly that rect, clips it, and the tile path renders 1:1
    // in document-local units.
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    let frame = &src[src
        .find("checker-frame := Rectangle {")
        .ok_or("the checker frame is gone")?..];
    let frame = &frame[..frame
        .find("Image {")
        .ok_or("no board image after the frame")?];
    for binding in [
        "x: root.checker-x;",
        "y: root.checker-y;",
        "width: root.checker-w;",
        "height: root.checker-h;",
    ] {
        assert!(
            frame.contains(binding),
            "the checker frame binds `{binding}`"
        );
    }
    assert!(
        frame.contains("clip: true;"),
        "the checker frame clips the tiles to the document"
    );
    assert!(
        frame.contains("commands: root.checker;") && frame.contains("fit: preserve;"),
        "the tile path renders 1:1 inside the document frame"
    );
    Ok(())
}

/// The checker is exactly the document, at 100%, panned and zoomed: every pixel
/// inside the document rectangle is a checker tile, every pixel of the ring just
/// outside it is the canvas surround, and the painted bounds are the document
/// bounds.
#[test]
fn the_checker_paints_exactly_the_document_rect() -> TestResult {
    init_backend_allow_existing();
    let palette = palette(false)?;
    for (view_x, view_y, zoom) in [(4.0, 4.0, 1.0), (137.0, 63.0, 1.0), (250.0, 120.0, 2.0)] {
        let view = crate::image::ImageView {
            x: view_x,
            y: view_y,
            k: zoom,
        };
        let frame = render_board(200.0, 120.0, Some(view), false)?;
        let (dx, dy, dw, dh) = frame.doc;

        let x0 = u32::try_from(crate::vm::f2i(dx)).unwrap_or(0);
        let y0 = u32::try_from(crate::vm::f2i(dy)).unwrap_or(0);
        let x1 = u32::try_from(crate::vm::f2i(dx + dw)).unwrap_or(0);
        let y1 = u32::try_from(crate::vm::f2i(dy + dh)).unwrap_or(0);
        assert!(
            (dx - f64::from(x0)).abs() < 1e-9 && (dy - f64::from(y0)).abs() < 1e-9,
            "the pinned view puts the document on whole pixels ({view:?})"
        );

        // The two tile tones, read from the first light and dark 8px cell.
        let light = pixel_at(&frame.buffer, x0 + 3, y0 + 3);
        let dark = pixel_at(&frame.buffer, x0 + 11, y0 + 3);
        assert!(
            near(light, palette.checker_light, 2),
            "the light tile composites to the theme tone ({light:?} at {view:?})"
        );
        assert!(
            near(dark, palette.checker_dark, 2),
            "the dark tile composites to the theme tone ({dark:?} at {view:?})"
        );
        assert_ne!(light, dark, "the tiles are distinguishable");

        // Inside: only checker, everywhere — the plate reaches every edge.
        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = pixel_at(&frame.buffer, x, y);
                assert!(
                    pixel == light || pixel == dark,
                    "({x}, {y}) inside the document is {pixel:?}, not a checker tile at {view:?}"
                );
            }
        }
        // Outside: the one-pixel ring is the canvas surround, exactly.
        for x in (x0 - 1)..=x1 {
            for y in [y0 - 1, y1] {
                assert_eq!(
                    pixel_at(&frame.buffer, x, y),
                    palette.canvas,
                    "({x}, {y}) just outside the document is not the surround at {view:?}"
                );
            }
        }
        for y in y0..y1 {
            for x in [x0 - 1, x1] {
                assert_eq!(
                    pixel_at(&frame.buffer, x, y),
                    palette.canvas,
                    "({x}, {y}) just outside the document is not the surround at {view:?}"
                );
            }
        }
        // The painted bounds are the document bounds: the outermost row and
        // column inside are checker, in every corner and at the edge midpoints.
        for x in [x0, x0 + (x1 - x0) / 2, x1 - 1] {
            for y in [y0, y1 - 1] {
                let pixel = pixel_at(&frame.buffer, x, y);
                assert!(
                    pixel == light || pixel == dark,
                    "the document edge at ({x}, {y}) is {pixel:?} at {view:?}"
                );
            }
        }
        for y in [y0, y0 + (y1 - y0) / 2, y1 - 1] {
            for x in [x0, x1 - 1] {
                let pixel = pixel_at(&frame.buffer, x, y);
                assert!(
                    pixel == light || pixel == dark,
                    "the document edge at ({x}, {y}) is {pixel:?} at {view:?}"
                );
            }
        }
    }
    Ok(())
}

/// The selection and transform overlays stay screen-space and inside the
/// document: the marching ants trace the selection where it is on screen (they
/// used to be fitted into the board), and the doc frame clips them.
#[test]
fn the_marching_ants_trace_the_selection_inside_the_document() -> TestResult {
    init_backend_allow_existing();
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Image;
        app.image.project.width = 100.0;
        app.image.project.height = 100.0;
        app.image.project.board_layers.clear();
        app.image.show_rulers = false;
        app.image.show_grid = false;
        app.image.show_guides = false;
        app.image.canvas = (800.0, 600.0);
        app.image.fitted = "ants:100x100".to_string();
        app.image.view = crate::image::ImageView {
            x: 100.0,
            y: 50.0,
            k: 1.0,
        };
        // A 60x60 selection inside the document, in document pixels.
        let mut mask = open_canvas_ps::Surface::new(100, 100);
        for y in 20..80 {
            for x in 20..80 {
                mask.set(i64::from(x), i64::from(y), [255, 255, 255, 255]);
            }
        }
        app.image.selection = Some(mask);
        app.sync_image(&ui);
    });
    let ants = ui.get_image_ants().to_string();
    assert!(!ants.is_empty(), "the selection paints marching ants");
    // Every ant vertex is inside the document rect on screen: document (0,0) is
    // board (100, 50), the selection is document (20..80, 20..80).
    let mut points = 0;
    for pair in ants.split('L').skip(1) {
        let mut values = pair
            .split_whitespace()
            .filter_map(|token| token.parse::<f64>().ok());
        let (Some(x), Some(y)) = (values.next(), values.next()) else {
            continue;
        };
        points += 1;
        assert!(
            (119.0..=181.0).contains(&x) && (69.0..=131.0).contains(&y),
            "an ant vertex sits on the selection, not on a fitted box ({x}, {y})"
        );
    }
    assert!(
        points > 20,
        "the ants trace the whole selection ring ({points})"
    );

    ui.set_view("image".into());
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;

    // The ants' dark parity paints on the selection's top edge, in window
    // coordinates: the board sits directly under the menu bar and right of the
    // rail.
    let canvas = (
        f64::from(ui.get_image_canvas_width()),
        f64::from(ui.get_image_canvas_height()),
    );
    let board_x = u32::try_from(crate::vm::f2i(1280.0 - canvas.0)).unwrap_or(0);
    let board_y = u32::try_from(crate::vm::f2i(800.0 - 24.0 - canvas.1)).unwrap_or(0);
    let selection_top = board_y + 50 + 20;
    // The ants' dark parity dashes (4 on, 4 off) sit on the selection's top
    // edge; the light parity alternates with them.
    let mut dark_dashes = 0;
    for y in (selection_top - 1)..=(selection_top + 1) {
        for x in (board_x + 100 + 21)..(board_x + 100 + 79) {
            let p = pixel_at(&buffer, x, y);
            if u32::from(p.0) < 150 && u32::from(p.1) < 150 && u32::from(p.2) < 150 {
                dark_dashes += 1;
            }
        }
    }
    assert!(
        dark_dashes >= 16,
        "the ants paint along the selection's top edge ({dark_dashes} px, row {selection_top})"
    );
    Ok(())
}

/// No selection-like border on the document. With an active layer covering the
/// board (the everyday case), the document edge paints the document tone, the
/// checker or the surround — never the selection-blue stroke that used to trace
/// the active layer's box. Nothing in the board is blue at all.
#[test]
fn the_document_carries_no_blue_outline() -> TestResult {
    init_backend_allow_existing();
    let palette = palette(false)?;
    let view = crate::image::ImageView {
        x: 520.0,
        y: 261.0,
        k: 1.0,
    };
    let frame = render_board(200.0, 120.0, Some(view), true)?;
    let (dx, dy, dw, dh) = frame.doc;
    let x0 = u32::try_from(crate::vm::f2i(dx)).unwrap_or(0);
    let y0 = u32::try_from(crate::vm::f2i(dy)).unwrap_or(0);
    let x1 = u32::try_from(crate::vm::f2i(dx + dw)).unwrap_or(0);
    let y1 = u32::try_from(crate::vm::f2i(dy + dh)).unwrap_or(0);

    let is_blue = |pixel: (u8, u8, u8)| {
        u32::from(pixel.2) > u32::from(pixel.0) + 25 && u32::from(pixel.2) > u32::from(pixel.1) + 10
    };

    // The document's edge ring: no blue stroke, and the edge itself is the
    // document (the transparent layer shows the checker through).
    let mut blue = 0;
    for x in x0..x1 {
        for y in [y0, y1 - 1] {
            if is_blue(pixel_at(&frame.buffer, x, y)) {
                blue += 1;
            }
        }
    }
    for y in y0..y1 {
        for x in [x0, x1 - 1] {
            if is_blue(pixel_at(&frame.buffer, x, y)) {
                blue += 1;
            }
        }
    }
    assert_eq!(blue, 0, "the document edge carries no blue stroke");
    assert!(
        near(
            pixel_at(&frame.buffer, x0 + 5, y0),
            palette.checker_light,
            2
        ),
        "the top edge paints the document's checker tile, not a border"
    );

    // Stronger: the whole board is free of selection-blue ink. The old overlay
    // fitted the active layer's box into the board, which painted exactly this
    // kind of stray blue rectangle.
    let (board_x, board_y, board_w, board_h) = frame.board;
    let bx = u32::try_from(crate::vm::f2i(board_x)).unwrap_or(0);
    let by = u32::try_from(crate::vm::f2i(board_y)).unwrap_or(0);
    let bx1 = u32::try_from(crate::vm::f2i(board_x + board_w)).unwrap_or(0);
    let by1 = u32::try_from(crate::vm::f2i(board_y + board_h)).unwrap_or(0);
    let mut blue_in_board = 0;
    for y in by..by1.min(frame.buffer.height()) {
        for x in bx..bx1.min(frame.buffer.width()) {
            if is_blue(pixel_at(&frame.buffer, x, y)) {
                blue_in_board += 1;
            }
        }
    }
    assert_eq!(
        blue_in_board, 0,
        "nothing in the board paints in selection blue"
    );
    Ok(())
}
