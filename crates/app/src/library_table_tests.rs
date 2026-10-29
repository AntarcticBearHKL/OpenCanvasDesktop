//! Headless render + source tests for the home screen's model (Projects hold
//! Canvases) and its flat two-pane layout: the ~240px project sidebar on the
//! left, the right pane's single header row (project name, ghost `+`, drag
//! surface, settings, window controls) and the full-width canvas table below.
//! Rows paint, the selected-row highlight differs from the resting row, the
//! trailing action icons sit on the row's centre line, and the two empty states
//! (no projects / empty project) are distinct.
//! Code-only verification (project rule): the Slint testing backend embeds the
//! software rasterizer, so `Window::take_snapshot` returns a real pixel buffer.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model as _};

use open_canvas_core::document::{
    CanvasGroup, CanvasProject, ProjectIndexEntry, PROJECT_KIND_CANVAS,
};

use crate::app::View;
use crate::store::Store;
use crate::test_support::{differing_pixels, init_backend_allow_existing};
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Buffer = slint::SharedPixelBuffer<slint::Rgba8Pixel>;

/// One wired home screen on a fresh temp store: two projects, one canvas each,
/// the `selected` project selected.
type Home = (tempfile::TempDir, AppWindow, Rc<RefCell<crate::app::App>>);

fn home(selected: &str) -> Result<Home, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Library;
        app.library.index.groups = vec![
            CanvasGroup {
                id: "g0".into(),
                name: "Project 1".into(),
                created_at: String::new(),
            },
            CanvasGroup {
                id: "g1".into(),
                name: "Project 2".into(),
                created_at: String::new(),
            },
        ];
        app.library.index.projects = vec![entry("c0", "g0"), entry("c1", "g1")];
        app.library.selected_group = Some(selected.into());
        app.sync_library(&ui);
    });
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok((dir, ui, app))
}

/// One wired home screen whose canvases exist on disk, so a row's open action
/// can really enter the editor (the plain render fixture only paints rows).
fn clickable_home() -> Result<Home, Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    {
        let mut app = app.borrow_mut();
        app.view = View::Library;
        app.library.index.groups = vec![CanvasGroup {
            id: "g0".into(),
            name: "Project 1".into(),
            created_at: String::new(),
        }];
        for (id, title) in [("c0", "Canvas 0"), ("c1", "Canvas 1")] {
            let mut canvas = CanvasProject::new(id, title, String::new(), "2026-02-01T09:30:00Z");
            canvas.group_id = Some("g0".to_string());
            app.store.save_project(&canvas)?;
        }
        app.library.index.projects = vec![entry("c0", "g0"), entry("c1", "g0")];
        app.library.selected_group = Some("g0".into());
        app.sync_library(&ui);
    }
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    Ok((dir, ui, app))
}

fn entry(id: &str, project: &str) -> ProjectIndexEntry {
    ProjectIndexEntry {
        id: id.into(),
        title: format!("Canvas {}", id.trim_start_matches('c')),
        created_at: String::new(),
        // A real stamp keeps the date column painted (it shows `MM-DD HH:MM`),
        // so the centre-line probes have text in all three cells.
        updated_at: "2026-02-01T09:30:00Z".into(),
        group_id: Some(project.into()),
        kind: PROJECT_KIND_CANVAS.into(),
    }
}

/// One `(r, g, b)` of the snapshot, or `None` out of bounds.
type Rgb = (u8, u8, u8);
/// One `(x, y)` position, or a `(first, last)` range of them.
type Point = (u32, u32);

/// One pixel of the snapshot, or `None` out of bounds.
fn pixel_at(buffer: &Buffer, x: u32, y: u32) -> Option<Rgb> {
    let width = usize::try_from(buffer.width()).ok()?;
    let index = usize::try_from(y).ok()? * width + usize::try_from(x).ok()?;
    buffer
        .as_slice()
        .get(index)
        .map(|pixel| (pixel.r, pixel.g, pixel.b))
}

/// The one surface tone the window plate paints behind the home screen.
fn surface_tone(ui: &AppWindow) -> Rgb {
    let background = ui.global::<crate::Theme>().get_background();
    (background.red(), background.green(), background.blue())
}

fn near(got: Rgb, want: Rgb) -> bool {
    got.0.abs_diff(want.0) <= 2 && got.1.abs_diff(want.1) <= 2 && got.2.abs_diff(want.2) <= 2
}

/// Whether `pixel` carries ink over `tone` (text or a glyph, not the wash the
/// cell paints): summed channel distance above antialiasing noise.
fn inked(pixel: Rgb, tone: Rgb) -> bool {
    let distance = u32::from(pixel.0.abs_diff(tone.0))
        + u32::from(pixel.1.abs_diff(tone.1))
        + u32::from(pixel.2.abs_diff(tone.2));
    distance > 24
}

/// The first and last scan line of `region` that carries ink over `tone`, or
/// `None` when the region is blank. Probes a cell's painted content, so the
/// cell's own wash (hover / selected tint) is the reference tone.
fn ink_span(buffer: &Buffer, (x0, x1): Point, (y0, y1): Point, tone: Rgb) -> Option<Point> {
    let mut first = u32::MAX;
    let mut last = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(pixel) = pixel_at(buffer, x, y) {
                if inked(pixel, tone) {
                    first = first.min(y);
                    last = last.max(y);
                }
            }
        }
    }
    (first != u32::MAX).then_some((first, last))
}
/// The inked columns of `region`, grouped into glyph clusters separated by
/// more than five blank columns.
fn ink_clusters(buffer: &Buffer, (x0, x1): Point, (y0, y1): Point, tone: Rgb) -> Vec<Point> {
    let mut clusters: Vec<Point> = Vec::new();
    for x in x0..x1 {
        let inked_column =
            (y0..y1).any(|y| pixel_at(buffer, x, y).is_some_and(|pixel| inked(pixel, tone)));
        if !inked_column {
            continue;
        }
        match clusters.last_mut() {
            Some((_, last)) if x.saturating_sub(*last) <= 5 => *last = x,
            _ => clusters.push((x, x)),
        }
    }
    clusters
}

/// The vertical centre of an ink span.
fn span_centre((first, last): Point) -> f32 {
    f32::midpoint(
        f32::from(u16::try_from(first).unwrap_or(u16::MAX)),
        f32::from(u16::try_from(last).unwrap_or(u16::MAX)),
    )
}
/// Renders the home screen with `projects` projects in the sidebar and `n`
/// canvases in the first one. With `select` the first canvas row is selected;
/// with `no_project` the project selection is cleared (the "create a project"
/// state).
fn render_library_ui(
    projects: usize,
    n: usize,
    select: bool,
    no_project: bool,
) -> Result<(AppWindow, Buffer), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let ui = AppWindow::new()?;
    let store = Store::at(dir.path().to_path_buf());
    let app = Rc::new(RefCell::new(crate::app::App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| {
        app.view = View::Library;
        app.library.index.groups = (0..projects)
            .map(|i| CanvasGroup {
                id: format!("g{i}"),
                name: format!("Project {}", i + 1),
                created_at: String::new(),
            })
            .collect();
        app.library.index.projects = (0..n).map(|i| entry(&format!("c{i}"), "g0")).collect();
        app.library.selected_group = if no_project { None } else { Some("g0".into()) };
        if select {
            app.library.selected_project = Some("c0".into());
        }
        app.sync_library(&ui);
    });
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.show()?;
    ui.window().request_redraw();
    let buffer = ui.window().take_snapshot()?;
    Ok((ui, buffer))
}

fn render_library(
    n: usize,
    select: bool,
    no_project: bool,
) -> Result<Buffer, Box<dyn std::error::Error>> {
    render_library_ui(1, n, select, no_project).map(|(_, buffer)| buffer)
}

/// A left-button click at a logical window position (press + release), the way
/// the platform delivers it.
fn click_at(ui: &AppWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};

    let position = slint::LogicalPosition::new(x, y);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

#[test]
fn canvas_rows_paint_below_the_header() -> TestResult {
    init_backend_allow_existing();
    let empty = render_library(0, false, false)?;
    let three = render_library(3, false, false)?;
    let painted = differing_pixels(&empty, &three)?;
    assert!(
        painted > 3_000,
        "the three canvas rows paint over the empty-project state ({painted}px)"
    );
    Ok(())
}

#[test]
fn the_selected_canvas_row_is_visibly_highlighted() -> TestResult {
    init_backend_allow_existing();
    let resting = render_library(2, false, false)?;
    let selected = render_library(2, true, false)?;
    let painted = differing_pixels(&resting, &selected)?;
    assert!(
        painted > 200,
        "the selected row's tint and weight paint ({painted}px)"
    );
    Ok(())
}

#[test]
fn the_no_project_and_empty_project_states_are_distinct() -> TestResult {
    init_backend_allow_existing();
    let no_project = render_library(0, false, true)?;
    let empty_project = render_library(0, false, false)?;
    let painted = differing_pixels(&no_project, &empty_project)?;
    assert!(
        painted > 1_000,
        "the two empty states read differently ({painted}px)"
    );
    Ok(())
}

/// The sidebar is a real ~240px column: its separator hairline paints at the
/// 240px edge while the right pane next to it stays the one surface tone on the
/// same scan line. The rest of the window is not a card plate.
#[test]
fn the_project_sidebar_is_a_240px_column_on_the_same_surface() -> TestResult {
    init_backend_allow_existing();
    let (ui, buffer) = render_library_ui(3, 3, false, false)?;
    let base = surface_tone(&ui);
    let width = buffer.width();
    // A scan line through the sidebar surface and the table's column header:
    // the separator sits at 239px, the right pane's own column at 240+ is
    // surface again.
    let y = 110;
    let separator = pixel_at(&buffer, 239, y).ok_or("sidebar separator probe")?;
    assert!(
        !near(separator, base),
        "the 240px sidebar draws its separator (read {separator:?}, surface {base:?})"
    );
    let pane = pixel_at(&buffer, 300, y).ok_or("right pane probe")?;
    assert!(
        near(pane, base),
        "the right pane stays on the one window surface ({pane:?})"
    );
    assert!(width > 600, "the snapshot keeps its desktop width");
    Ok(())
}

/// The canvas table spans the entire right pane: the header rule and every row
/// separator start at the pane's left edge (the 240px sidebar seam) and run to
/// the window's right edge, so header cells and rows share one full-width
/// geometry instead of a centred block.
#[test]
fn the_canvas_table_spans_the_right_pane_edge_to_edge() -> TestResult {
    init_backend_allow_existing();
    let (ui, buffer) = render_library_ui(1, 3, false, false)?;
    let base = surface_tone(&ui);
    let width = buffer.width();
    let height = buffer.height();
    // The right pane starts right after the 240px sidebar (and its 1px seam);
    // the table rules run to the last pixel of the window.
    let pane_left = 240u32;
    let pane_right = width - 1;
    let mut extents: Vec<(u32, u32)> = Vec::new();
    for y in 100..height.saturating_sub(2) {
        let centre = pixel_at(&buffer, width / 2, y).ok_or("centre probe")?;
        if near(centre, base) {
            continue;
        }
        // A table rule is one flat tone across the whole pane; glyph rows are
        // not equal at four distant probes.
        let a = pixel_at(&buffer, 400, y).ok_or("rule probe")?;
        let b = pixel_at(&buffer, 800, y).ok_or("rule probe")?;
        let c = pixel_at(&buffer, 1100, y).ok_or("rule probe")?;
        if a != centre || b != centre || c != centre {
            continue;
        }
        let mut first = width;
        let mut last = 0u32;
        for x in pane_left..width {
            let probe = pixel_at(&buffer, x, y).ok_or("edge probe")?;
            if !near(probe, base) {
                first = first.min(x);
                last = last.max(x);
            }
        }
        extents.push((first, last));
    }
    assert!(
        extents.len() >= 3,
        "the header rule and one separator per row run across the pane ({})",
        extents.len()
    );
    for (first, last) in &extents {
        assert!(
            first.abs_diff(pane_left) <= 2,
            "the table starts at the pane's left edge ({first}, want ~{pane_left})"
        );
        assert!(
            last.abs_diff(pane_right) <= 2,
            "the table reaches the window's right edge ({last}, want ~{pane_right})"
        );
    }
    // Header and rows share the exact same geometry: one common extent.
    let (first0, last0) = extents[0];
    assert!(
        extents
            .iter()
            .all(|(first, last)| first.abs_diff(first0) <= 1 && last.abs_diff(last0) <= 1),
        "every table rule shares the header's full-width geometry ({extents:?})"
    );
    Ok(())
}

/// Source guard for the reworked layout: a two-pane horizontal root (240px
/// project sidebar + the right pane), the 76px app band above both, the 44px
/// header stack, the full-width table, and no opaque card/panel plate anywhere.
#[test]
fn the_home_source_has_the_project_sidebar_and_full_width_table() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    for token in ["glass-panel", "Theme.card", "shadow", "library-eyebrow"] {
        assert!(
            !home.contains(token),
            "the flat home screen still carries the {token} token"
        );
    }
    assert!(
        home.contains("background: transparent;"),
        "the screen paints no plate of its own; the window surface shows through"
    );
    assert!(
        home.contains("width: 240px;"),
        "the project sidebar is the ~240px column"
    );
    assert!(
        home.contains("component ProjectSidebarRow inherits Rectangle"),
        "projects are sidebar rows"
    );
    assert!(
        home.contains("component CanvasRow inherits Rectangle"),
        "canvases are table rows"
    );
    assert!(
        home.contains("icon: Icons.home;") && !home.contains("I18n.library-projects-label"),
        "the sidebar starts with Home, not a PROJECTS wordmark"
    );
    assert!(
        !home.contains("min(860px") && !home.contains("860px"),
        "the table is no longer capped at 860px"
    );
    assert!(
        !home.contains("x: (parent.width - self.width) / 2;"),
        "the table block is no longer centred in the pane"
    );
    // The column share is identical in the header and in every row: the name
    // and last-edited cells are exact fractions of the shared block width, the
    // actions cell takes the rest, so header and row columns line up pixel for
    // pixel (a pure stretch would shift them by their preferred text widths).
    for share in [
        "(parent.width - 32px) * 0.4;",
        "(parent.width - 32px) * 0.28;",
    ] {
        assert_eq!(
            home.matches(share).count(),
            2,
            "the header and the rows share the {share} column"
        );
    }
}

/// The home screen has three horizontal bands, in this order:
///
/// * the app band: PROJECTS at the far left, the settings gear and the three
///   window controls at the far right, and the drag surface filling the middle;
/// * the app band's ghost `+` New Project, beside the PROJECTS title;
/// * the right pane's project header row (`Project 1` + its ghost `+`) above the
///   table's column header.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one test pins the app band, the project header and the sidebar chrome contract"
)]
fn the_app_band_carries_the_title_and_the_window_controls() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    let band_start = home
        .find("// 0. the app band")
        .expect("the app band marker");
    let band_end = home
        .find("// 1. the project sidebar")
        .expect("the sidebar follows the app band");
    let band = &home[band_start..band_end];

    let home_icon = band
        .find("icon: Icons.home;")
        .expect("the app band starts with the Home house");
    let settings = band
        .find("icon: Icons.settings;")
        .expect("the band carries the settings gear");
    let controls = band
        .find("WindowButtons {")
        .expect("the band carries the window controls");
    assert!(
        home_icon < settings && settings < controls,
        "Home, then the gear, then minimise/maximise/close: one band in reading order"
    );
    assert!(
        !band.contains("I18n.library-projects-label"),
        "the PROJECTS wordmark is gone from the app band"
    );
    assert!(
        band.contains("WindowDragArea {")
            && band.contains("root.drag-begin();")
            && band.contains("root.drag-tick();")
            && band.contains("root.drag-end();")
            && band.contains("root.maximize-toggle();"),
        "the band is the manual drag surface and double-clicks to maximise"
    );
    assert!(
        band.contains("height: root.title-band;"),
        "the band is the shared title-band token, not a stray literal"
    );

    // The right pane's header: the ghost + alone. No project name is written
    // to its left; the selected project is highlighted in the sidebar.
    let header_start = home
        .find("// ---- the project header row")
        .expect("the right header marker");
    let header_end = home
        .find("// ---- the canvas table area")
        .expect("the header row is followed by the table area");
    let header = &home[header_start..header_end];
    let plus = header
        .find("tip: I18n.library-new-canvas;")
        .expect("the header carries the New Canvas tooltip");
    let create = header
        .find("root.create-project();")
        .expect("the header's + adds a canvas to the selected project");
    assert!(
        plus < create,
        "the + is the header's only action; no project name sits to its left"
    );
    assert!(
        !header.contains("root.group-title") && !header.contains("I18n.library-no-project"),
        "the project name is not repeated in the right header"
    );
    assert!(
        !header.contains("WindowDragArea {")
            && !header.contains("Icons.settings;")
            && !header.contains("WindowButtons {"),
        "the window drag/gear/controls live only in the app band"
    );
    assert!(
        header.contains("height: root.project-header-band;"),
        "the project title takes the 26px top half of the 44px header stack"
    );

    // New Project is beside the PROJECTS title in the app band; the sidebar
    // no longer reserves a 44px empty toolbar for it.
    let new_project = band
        .find("tip: I18n.library-new-project;")
        .expect("the app band offers New Project");
    let project_block_start = band[..new_project]
        .rfind("IconButton {")
        .expect("New Project is an IconButton");
    let project_block = &band[project_block_start..];
    assert!(
        project_block.contains("variant: \"ghost\"")
            && project_block.contains("icon: Icons.plus;")
            && project_block.contains("root.create-group();"),
        "New Project is a ghost + icon button wired to create-group"
    );
    assert!(
        home_icon < new_project && new_project < settings,
        "Home, then New Project, then the gear: one reading order"
    );
    let sidebar = &home[band_end..header_start];
    assert!(
        !sidebar.contains("I18n.library-projects-label"),
        "the PROJECTS label moved to the app band and is not duplicated in the sidebar"
    );
    assert!(
        !sidebar.contains("tip: I18n.library-new-project;"),
        "New Project moved out of the empty sidebar toolbar"
    );
    assert!(
        !sidebar.contains("Icons.folder-plus"),
        "the old folder-plus glyph is gone from the sidebar"
    );
    assert!(
        !sidebar.contains("I18n.library-new-canvas") && !sidebar.contains("root.create-project()"),
        "the sidebar never creates a canvas; that lives in the right pane"
    );
    // The create action is a plain ghost `+`, not a filled label button.
    assert!(
        !header.contains("label: I18n.library-new-canvas;"),
        "the filled New Canvas button is gone from the header"
    );
}

/// The header's column cells and the row's action cell all fill the row height
/// and centre their content vertically, so the table header cannot sit off the
/// action icons any more.
#[test]
fn the_header_cells_and_actions_are_vertically_centred() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    let header_start = home
        .find("// ---- column header (same 40 / 28 / 32 share) -----")
        .expect("the column header marker");
    let header_end = home
        .find("// ---- canvas rows, or the empty-project state -----")
        .expect("the column header ends before the rows");
    let header = &home[header_start..header_end];
    for label in [
        "I18n.library-name-column",
        "I18n.library-updated-column",
        "I18n.library-actions-column",
    ] {
        assert!(header.contains(label), "the header names {label}");
    }
    assert_eq!(
        header.matches("(parent.width - 32px)").count(),
        2,
        "the name and last-edited cells are exact shares of the block"
    );
    assert_eq!(
        header.matches("horizontal-stretch: 1;").count(),
        1,
        "the actions cell takes the remaining third"
    );
    assert!(
        header.matches("height: 100%;").count() >= 6,
        "each header cell fills the row height and its label fills the cell"
    );
    assert_eq!(
        header.matches("vertical-alignment: center;").count(),
        3,
        "all three header labels are vertically centred"
    );
    assert!(
        header.contains("cross-axis-alignment: center;"),
        "the header row centres its cells on the cross axis"
    );

    let actions_start = home
        .find("// ---- actions (32%, icons centred in the cell) ----")
        .expect("the actions cell marker");
    let actions = &home[actions_start..];
    assert!(
        actions.contains("height: 100%;")
            && actions.contains("alignment: center;")
            && actions.contains("cross-axis-alignment: center;"),
        "the action icons are centred in a full-height cell on both axes"
    );
}

/// Render probe: the trailing rename / duplicate / delete icons sit on the
/// same centre line as the row's name and date text, and the header's Actions
/// label sits on the centre line of its own strip.
#[test]
fn the_row_action_icons_share_the_row_centre_line() -> TestResult {
    init_backend_allow_existing();
    let (ui, buffer) = render_library_ui(1, 2, true, false)?;
    let width = buffer.width();
    assert!(
        width > 1_000,
        "the desktop snapshot is wide enough to probe"
    );
    // 76px app band + 44px header stack; rows are 44px tall.
    let row_top = 120u32;
    let row_centre = row_top + 22;
    // The selected row's wash is the ink reference: sample the row's left
    // padding, above the text.
    let tone = pixel_at(&buffer, 246, row_top + 2).ok_or("row tone probe")?;
    // Row 0's cells: name 40%, last edited 28%, actions 32% after a 16px inset
    // on each side (256..1264 in a 1280px window).
    for (label, (x0, x1)) in [
        ("name", (260u32, 640u32)),
        ("last edited", (670, 930)),
        ("actions", (950, 1270)),
    ] {
        let span = ink_span(&buffer, (x0, x1), (row_top + 2, row_top + 42), tone)
            .unwrap_or_else(|| panic!("the {label} cell paints"));
        let centre = span_centre(span);
        let want = f32::from(u16::try_from(row_centre).unwrap_or(0));
        assert!(
            (centre - want).abs() <= 3.0,
            "the {label} content is centred on the row ({span:?}, centre {centre}, want {want})"
        );
        // The trailing icons are one group, icon-sized like the row text.
        if label == "actions" {
            let span_height = span.1.abs_diff(span.0);
            assert!(
                (6..=18).contains(&span_height),
                "the actions group is one line of icon glyphs ({span_height}px tall)"
            );
        }
    }

    // The header strip: the ACTIONS label is centred in the 18px column-header
    // band (102..120), exactly like the row icons are centred in theirs.
    let header_tone = surface_tone(&ui);
    let header_centre = 102u32 + 9;
    let span = ink_span(&buffer, (950, 1270), (103, 119), header_tone)
        .ok_or("the Actions header label paints")?;
    let centre = span_centre(span);
    let want = f32::from(u16::try_from(header_centre).unwrap_or(0));
    assert!(
        (centre - want).abs() <= 3.0,
        "the Actions label shares the header strip's centre line ({centre}, want {want})"
    );
    Ok(())
}

/// The row's trailing cluster is exactly the three remaining icons: removing
/// the open button leaves rename / duplicate / delete and nothing else.
#[test]
fn the_row_draws_three_action_icons_and_no_open_button() -> TestResult {
    init_backend_allow_existing();
    let (_ui, buffer) = render_library_ui(1, 1, true, false)?;
    let row_top = 120u32;
    let tone = pixel_at(&buffer, 246, row_top + 2).ok_or("row tone probe")?;
    let clusters = ink_clusters(&buffer, (950, 1270), (row_top + 2, row_top + 42), tone);
    assert_eq!(
        clusters.len(),
        3,
        "the actions cell paints three glyphs (rename / duplicate / delete): {clusters:?}"
    );
    // Each glyph is an icon-sized cluster, not a text label.
    for (first, last) in &clusters {
        assert!(
            (4..=18).contains(&last.abs_diff(*first)),
            "one icon glyph ({first}..{last})"
        );
    }
    // The group is centred in the actions cell, so dropping the open button
    // moved the group right by half a button instead of leaving a gap.
    let group_centre = span_centre((clusters[0].0, clusters[clusters.len() - 1].1));
    let cell_centre = 1_102.0;
    assert!(
        (group_centre - cell_centre).abs() <= 4.0,
        "the three icons stay centred in the actions cell ({group_centre}, want {cell_centre})"
    );
    Ok(())
}

/// Source guard: the row offers rename / duplicate / delete and opens on a
/// single click; the fullscreen/open icon is gone.
#[test]
fn the_row_action_list_drops_open_and_keeps_the_three_actions() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    let start = home
        .find("component CanvasRow inherits Rectangle")
        .expect("the canvas row component");
    let end = home
        .find("// The app band's drag surface")
        .expect("the row component ends before the header helpers");
    let row = &home[start..end];

    for retired in ["Icons.maximize", "Icons.fullscreen"] {
        assert!(
            !row.contains(retired),
            "the row no longer carries the {retired} open affordance"
        );
    }
    // `open` survives only as the single-click path, never as a button action.
    assert_eq!(
        row.matches("root.action(\"open\")").count(),
        1,
        "the single click is the row's only open dispatch"
    );
    assert!(
        row.contains("clicked =>") && row.contains("root.action(\"open\")"),
        "the single click dispatches open"
    );
    for (icon, action) in [
        ("Icons.pencil", "rename"),
        ("Icons.copy", "duplicate"),
        ("Icons.trash", "delete"),
    ] {
        assert!(
            row.contains(icon),
            "the row still paints the {action} glyph ({icon})"
        );
        assert!(
            row.contains(&format!("root.action(\"{action}\")")),
            "the row still binds {action} to its callback"
        );
    }
    // Three resting action buttons plus the editing save / cancel pair.
    assert_eq!(
        row.matches("IconButton {").count(),
        5,
        "the row carries three actions and the rename pair"
    );
}

/// Both create affordances are borderless, backgroundless `+` icons: the
/// sidebar header adds a project, the right pane header adds a canvas to the
/// selected one.
#[test]
fn both_create_actions_are_ghost_plus_buttons() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    let sidebar_end = home.find("// 2. the right pane").expect("sidebar block");
    let sidebar = &home[..sidebar_end];
    assert!(
        sidebar.contains("icon: Icons.plus;")
            && sidebar.contains("tip: I18n.library-new-project;")
            && sidebar.contains("root.create-group();"),
        "the PROJECTS header's + creates a project"
    );
    let header_end = home
        .find("// ---- the canvas table area")
        .expect("the header row");
    let header = &home[sidebar_end..header_end];
    assert!(
        header.contains("icon: Icons.plus;")
            && header.contains("tip: I18n.library-new-canvas;")
            && header.contains("root.create-project();"),
        "the project header's + adds a canvas to the selected project"
    );
    // Ghost variant: `bordered: false` and a transparent resting background,
    // so both + buttons are plain glyphs (the only wash is on hover).
    let widgets = include_str!("../ui/components/widgets.slint");
    assert!(
        widgets.contains("root.variant == \"ghost\" || root.variant == \"row\" || root.variant == \"menu\" ? false : true;"),
        "the ghost variant is defined as borderless"
    );
    assert!(
        widgets.contains(
            ": ta.has-hover && root.enabled ? (root.danger ? root.danger-hover-bg : root.hover-bg)"
        ) && widgets.contains(": transparent;"),
        "the ghost button's resting background is transparent"
    );

    // Source-level: both buttons are ghost glyphs; no pixel probe depends on
    // the removed project-name label in the header.
}

/// The two `+` callbacks reach the model: one adds a project, the other adds a
/// canvas to the selected project and opens it.
#[test]
fn both_plus_callbacks_create_a_project_and_a_canvas() -> TestResult {
    init_backend_allow_existing();
    let (_dir, ui, app) = home("g0")?;
    let groups_before = ui.get_library_groups().row_count();
    ui.invoke_library_create_group();
    assert_eq!(
        ui.get_library_groups().row_count(),
        groups_before + 1,
        "the PROJECTS + adds a project"
    );

    let canvases_before = crate::app::with_app_r(&app, |app| app.library.index.projects.len())
        .ok_or("the app state is borrowable")?;
    ui.invoke_library_create_project();
    crate::app::with_app(&app, |app| {
        assert_eq!(
            app.library.index.projects.len(),
            canvases_before + 1,
            "the header + adds a canvas to the selected project"
        );
        assert_eq!(app.view, View::Editor, "the new canvas opens");
    });
    Ok(())
}

/// A single click on a canvas row opens it; there is no double-click step.
#[test]
fn a_single_click_opens_the_canvas() -> TestResult {
    init_backend_allow_existing();
    // The first canvas row sits below the 76px app band and 44px header stack.
    let row = (500.0, 120.0 + 22.0);

    let (_open_dir, open_ui, open_app) = clickable_home()?;
    click_at(&open_ui, row.0, row.1);
    crate::app::with_app(&open_app, |app| {
        assert_eq!(app.view, View::Editor, "the single click opens the canvas");
        assert_eq!(
            app.editor.as_ref().map(|meta| meta.id.as_str()),
            Some("c0"),
            "the clicked row opens its own canvas"
        );
        assert_eq!(app.library.selected_project.as_deref(), Some("c0"));
    });
    Ok(())
}

/// A single click on a sidebar project switches the scope (selects the project)
/// and leaves the library up. Opening a canvas is the canvas row's single-click
/// action, not the project row's.
#[test]
fn a_single_click_on_a_project_switches_scope_without_opening_a_canvas() -> TestResult {
    init_backend_allow_existing();
    // The app band is 76px, each row 44px. With no empty toolbar, Project 2
    // is the second row, centred at 76 + 44 + 22.
    let (_dir, ui, app) = home("g0")?;
    click_at(&ui, 100.0, 142.0);
    crate::app::with_app(&app, |app| {
        assert_eq!(
            app.library.selected_group.as_deref(),
            Some("g1"),
            "the single click switches to Project 2"
        );
        assert!(
            app.editor.is_none(),
            "switching projects does not open a canvas"
        );
        assert_eq!(app.view, View::Library, "the library stays up");
    });
    Ok(())
}

/// Source guard: rows enter with a short token-bounded fade that honours reduced
/// motion, and the fade replays only for the rebuild in which Rust marks the
/// visible canvas set as changed.
#[test]
fn the_canvas_rows_fade_in_on_a_project_change() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    assert!(
        home.contains("animate enter {")
            && home.contains("duration: Theme.reduced-motion ? 0ms : Theme.motion-select;"),
        "the row enter fade uses the select token and respects reduced motion"
    );
    assert!(
        home.contains("in property <bool> fade-in: false;")
            && home.contains("in-out property <float> enter: root.fade-in ? 0.55 : 1;")
            && home.contains("fade-in: project.fade;"),
        "the fade is driven by the Rust-owned per-row fade flag"
    );
    assert!(
        !home.contains("Timer {") && !home.contains("changed epoch"),
        "the entrance needs no timer: the row's initial value chases 1 in init"
    );
    assert!(
        home.contains("duration: Theme.reduced-motion ? 0ms : (root.active ? Theme.motion-select : Theme.motion-hover);")
            && home.contains("duration: Theme.reduced-motion ? 0ms : (root.selected ? Theme.motion-select : Theme.motion-hover);"),
        "sidebar and canvas rows transition hover and selection on the motion tokens"
    );
    for long in ["Theme.motion-enter", "panel-motion", "motion-ghost"] {
        assert!(
            !home.contains(long),
            "the home screen adds no animation longer than its tokens ({long})"
        );
    }
}

/// Switching projects replays the row enter fade: the rows for the new project
/// start faint and settle on the select token (the testing backend's mock clock
/// drives the 150ms animation). A resting render of the same project is the
/// reference, so the only difference is the entrance opacity.
#[test]
fn switching_projects_replays_the_row_enter_fade() -> TestResult {
    init_backend_allow_existing();

    let (_dir, ui, app) = home("g0")?;
    let _first = ui.window().take_snapshot()?;
    // Force the switch through the real app path, which marks the rows.
    crate::app::with_app(&app, |app| app.library_select_group(&ui, "g1"));
    ui.window().request_redraw();
    let entering = ui.window().take_snapshot()?;

    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(200));
    ui.window().request_redraw();
    let settled = ui.window().take_snapshot()?;

    let (_reference_dir, reference_ui, _reference_app) = home("g1")?;
    let reference = reference_ui.window().take_snapshot()?;
    let fade = differing_pixels(&entering, &reference)?;
    assert!(
        fade > 200,
        "the switched project's rows start faint ({fade}px differ from rest)"
    );
    let settled_gap = differing_pixels(&settled, &reference)?;
    assert!(
        settled_gap <= 40,
        "the fade settles on the resting rows ({settled_gap}px)"
    );
    Ok(())
}

/// Source guard: the old folder copy is gone from the home screen and its i18n
/// table; everything the user reads says project (level 1) and canvas (level 2).
#[test]
fn the_home_copy_says_project_not_folder() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    let i18n_slint = include_str!("../ui/components/i18n.slint");
    let i18n_rs = include_str!("i18n.rs");
    for retired in [
        "New Folder",
        "No folder",
        "No folders yet",
        "library-empty-folder",
        "library-no-folder",
        "library.empty.folder",
        "library.empty.no-folder",
    ] {
        assert!(
            !home.contains(retired) && !i18n_slint.contains(retired) && !i18n_rs.contains(retired),
            "the home copy still says {retired}"
        );
    }
    for wanted in [
        "library-projects-label: \"PROJECTS\"",
        "library-new-project: \"New Project\"",
        "library-new-canvas: \"New Canvas\"",
        "library-empty-project: \"No canvases yet\"",
        "library-empty-no-projects: \"No projects yet\"",
    ] {
        assert!(
            i18n_slint.contains(wanted),
            "the i18n global carries the reworked copy ({wanted})"
        );
    }
    assert!(
        i18n_rs.contains("\"library.projects-label\"")
            && i18n_rs.contains("\"library.new-project\"")
            && i18n_rs.contains("\"library.new-canvas\""),
        "the Rust table carries the same keys"
    );
}

/// The app title sits in its own band above both columns, and the two lists
/// start on one line below it:
///
/// * PROJECTS is the 30px app title in the 76px band at the very top;
/// * there is no empty 44px sidebar toolbar, so the first project row starts
///   at 76, level with the right project title; the second project row starts
///   at 76 + 44 = 120;
/// * the right pane's header stack is 44px (a 26px project header row plus the
///   18px table column header), so the first canvas row also starts at 120 and
///   the second project row shares its centre line at 142.
#[test]
fn the_sidebar_label_and_the_first_rows_align_with_the_table() -> TestResult {
    init_backend_allow_existing();
    let (ui, buffer) = render_library_ui(3, 3, false, false)?;
    let base = surface_tone(&ui);

    // The table's column labels stay in the 102..120 column-header band.
    let columns =
        ink_span(&buffer, (670, 930), (103, 119), base).ok_or("the table's column labels paint")?;
    let columns_centre = span_centre(columns);
    assert!(
        (columns_centre - 111.0).abs() <= 2.0,
        "the column labels are centred in the 18px column-header band ({columns:?}, centre {columns_centre}, want 111)"
    );

    // With no empty sidebar toolbar, the first project row starts at the
    // content top (76), level with the right project header. The second project
    // row starts at 120, on the first canvas row; both 44px rows share 142.
    let first_project_row =
        ink_span(&buffer, (8, 200), (76, 118), base).ok_or("the first project row paints")?;
    let second_project_row =
        ink_span(&buffer, (8, 200), (120, 162), base).ok_or("the second project row paints")?;
    let canvas_row =
        ink_span(&buffer, (260, 640), (120, 162), base).ok_or("the first canvas row paints")?;
    assert!(
        first_project_row.0 < second_project_row.0,
        "the first project row sits above the second ({first_project_row:?}, {second_project_row:?})"
    );
    let project_centre = span_centre(second_project_row);
    let canvas_centre = span_centre(canvas_row);
    assert!(
        (project_centre - canvas_centre).abs() <= 1.5,
        "the second project row and Canvas 1 share a centre line          ({second_project_row:?} at {project_centre}, {canvas_row:?} at {canvas_centre})"
    );
    for (name, centre) in [
        ("second project row", project_centre),
        ("Canvas 1", canvas_centre),
    ] {
        assert!(
            (centre - 142.0).abs() <= 2.5,
            "the {name} is centred in its 44px row ({centre}, want 142)"
        );
    }
    Ok(())
}

/// Source guard for the alignment contract: the 76px app band sits above both
/// columns, there is no empty sidebar toolbar, the right header stack is
/// `header-band` (44px), its 26px project row plus the remaining column header
/// add up to it exactly, and both row components are 44px. Changing one side
/// alone breaks the pixel probe above.
#[test]
fn the_home_columns_share_equal_header_bands_and_44px_rows() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    assert!(
        home.contains("property <length> title-band: 76px;"),
        "the app band is a named 76px token"
    );
    assert!(
        home.contains("property <length> header-band: 44px;")
            && home.contains("property <length> project-header-band: 26px;"),
        "the 44px header stack and its 26px project row are named tokens"
    );
    assert_eq!(
        home.matches("height: root.header-band;").count(),
        0,
        "the empty sidebar toolbar is gone; the right header stack is the only 44px band"
    );
    assert!(
        home.contains("tip: I18n.library-new-project;"),
        "New Project lives in the app band"
    );
    assert!(
        home.contains("height: root.header-band - root.project-header-band;"),
        "the column header takes the rest of the 44px stack (44 - 26 = 18px)"
    );
    assert_eq!(
        home.matches("height: 44px;").count(),
        2,
        "the project rows and the canvas rows are both 44px"
    );
    assert!(
        !home.contains("height: 38px;") && !home.contains("table-header-band"),
        "the old 38px label band is gone"
    );
    let sidebar_rows = home
        .split("component ProjectSidebarRow inherits Rectangle")
        .nth(1)
        .and_then(|rest| rest.split("component CanvasRow inherits Rectangle").next())
        .expect("the project row component");
    let canvas_rows = home
        .split("component CanvasRow inherits Rectangle")
        .nth(1)
        .and_then(|rest| rest.split("// The app band's drag surface").next())
        .expect("the canvas row component");
    for (name, rows) in [("project", sidebar_rows), ("canvas", canvas_rows)] {
        assert!(rows.contains("height: 44px;"), "the {name} rows are 44px");
    }
}
