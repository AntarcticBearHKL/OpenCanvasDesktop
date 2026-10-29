//! Shell contract tests that do not need the Image Studio: the frosted surface
//! tiers every component resolves to, the top-bar/dock chrome wiring, the ruler
//! bitmaps, and the removal guards for retired features.
//!
//! The numeric measurements run on the live `Theme` token values, so a future
//! token edit fails here instead of silently flattening the stack.

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// No component keeps a superseded token, and every former user resolves to one
/// of the three frosted tiers: canvas field = content, top bar + side panel +
/// dock/toolbar + rulers = panel, menus/dialogs/popovers/toasts = strong. The
/// frost itself is the OS backdrop (`glass.rs`), never a captured image and
/// never a blurred bitmap in the UI tree.
#[test]
fn every_component_resolves_to_a_surface_tier() -> TestResult {
    use std::path::{Path, PathBuf};

    fn slint_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                slint_files(&path, out)?;
            } else if path.extension().is_some_and(|ext| ext == "slint") {
                out.push(path);
            }
        }
        Ok(())
    }

    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files = Vec::new();
    slint_files(&ui, &mut files)?;
    assert!(
        files.len() > 30,
        "the ui tree was walked ({} files)",
        files.len()
    );
    for path in files {
        // `theme.slint` names the retired tokens in the block that explains
        // them; every other file must be clean.
        if path.file_name().is_some_and(|name| name == "theme.slint") {
            continue;
        }
        let src = std::fs::read_to_string(&path)?;
        for needle in [
            "Theme.glass-surface",
            "Theme.glass-raised",
            "Theme.glass-highlight",
            "Theme.glass-hairline",
            "Theme.glass-shadow",
            "Theme.glass;",
            "Theme.glass-strong",
            "Theme.canvas-field",
            "Theme.window-bg",
            "glass-transparency",
            "\"transparency\"",
            "glass-resample",
            "tint-opacity",
            "transparentize(",
        ] {
            assert!(
                !src.contains(needle),
                "{} still uses the superseded token {needle}",
                path.display()
            );
        }
    }

    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    assert!(
        home.contains("background: transparent;"),
        "the home screen adds no plate of its own: it draws on the one window surface"
    );
    assert!(
        !home.contains("Theme.glass-panel"),
        "the home screen has no panel tier: one continuous surface"
    );
    assert!(
        include_str!("../ui/components/side-panel.slint")
            .replace("\r\n", "\n")
            .contains("background: Theme.glass-panel;"),
        "side panel = panel"
    );
    assert!(
        include_str!("../ui/components/floating.slint")
            .contains("background: Theme.glass-panel-strong;"),
        "menus = strong"
    );
    assert!(
        include_str!("../ui/components/modals.slint").contains("background: Theme.glass-panel;"),
        "dialogs = panel (transparent glass)"
    );
    assert!(
        include_str!("../ui/app.slint").contains("background: Theme.glass-content;"),
        "the canvas field = content"
    );
    Ok(())
}

/// The dock's side-panel toggle reaches `App::toggle_side_panel` (the home
/// screen has its own folder rail and no panel toggle).
#[test]
fn dock_panel_toggle_reaches_the_handler() -> TestResult {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    ui.set_view("editor".into());

    let before_ui = ui.get_side_panel_open();
    let before_app = app.borrow().side_open;
    ui.invoke_toggle_panel();
    assert_ne!(ui.get_side_panel_open(), before_ui, "the window flag flips");
    assert_ne!(app.borrow().side_open, before_app, "the app state flips");
    assert_eq!(
        ui.get_side_panel_open(),
        app.borrow().side_open,
        "the dock button and the app share one handler"
    );

    // And going home leaves the panel state alone.
    let open = ui.get_side_panel_open();
    crate::app::with_app(&app, |app| app.go_home(&ui));
    assert_eq!(
        ui.get_side_panel_open(),
        open,
        "home never collapses the side panel"
    );
    Ok(())
}

/// The side-panel header starts with Home and ends with the `+` that adds a
/// canvas project. There is no PROJECTS wordmark and no project name to the
/// left of the plus; the rows below carry the names.
#[test]
fn side_panel_header_offers_only_the_add_action() -> TestResult {
    let panel = include_str!("../ui/components/side-panel.slint").replace("\r\n", "\n");
    let app = include_str!("../ui/app.slint").replace("\r\n", "\n");

    // The header block: from its comment to the hairline under it.
    let header = panel
        .split("// Header:")
        .nth(1)
        .ok_or("the side panel has a header block")?
        .split("// The rows are")
        .next()
        .ok_or("the header ends before the rows")?;
    assert!(
        !header.contains("text: \"PROJECTS\";") && !header.contains("text: root.group-title;"),
        "the PROJECTS label and the project name are gone from the header"
    );
    assert_eq!(
        header.matches("IconButton {").count(),
        2,
        "the header carries Home and the create action"
    );
    assert!(
        header.contains("icon: Icons.home;")
            && header.contains("root.go-home();")
            && header.contains("icon: Icons.plus;")
            && header.contains("root.create-project();"),
        "Home then the + that creates a canvas"
    );
    assert!(
        !panel.contains("Icons.upload") && !panel.contains("import-project"),
        "the import/upload control and its callback are gone from the panel"
    );
    assert!(
        !app.contains("import-project =>") && !app.contains("root.side-panel-import();"),
        "the app no longer wires the panel's import action"
    );

    // The rows themselves are untouched.
    for wired in [
        "clicked => {\n                    root.open-project(row.id);",
        "root.project-action(row.id, action);",
        "root.reorder(row.id, direction);",
    ] {
        assert!(panel.contains(wired), "the panel row keeps {wired}");
    }
    Ok(())
}

/// Ctrl+V prefers a bitmap on the system clipboard: it becomes a real Image
/// node at the viewport centre instead of being ignored.
#[test]
fn clipboard_images_paste_as_image_nodes() {
    let src = include_str!("edit_events.rs");
    assert!(
        src.contains("system_clipboard::read_image()"),
        "paste reads a clipboard bitmap"
    );
    assert!(
        src.contains("insert_image_node(\"Pasted Image\""),
        "paste creates an image node from the bitmap"
    );
    assert!(
        src.contains("Surface::from_rgba"),
        "the clipboard RGBA is wrapped as a surface"
    );
}

/// Right-click is the universal "back" gesture for the shared modal shell and
/// the standalone settings modal.
#[test]
fn right_click_dismisses_modal_shells() {
    for (name, source) in [
        ("ModalShell", include_str!("../ui/components/modals.slint")),
        (
            "ConfigModal",
            include_str!("../ui/components/config-modal.slint"),
        ),
    ] {
        assert!(
            source
                .contains("e.kind == PointerEventKind.up && e.button == PointerEventButton.right"),
            "{name} handles right-click"
        );
        assert!(
            source.contains("root.close();"),
            "{name} forwards the dismiss"
        );
    }
}

/// A left-button drag on the side panel's background feeds the same manual
/// window move as the canvas background: `drag-begin` on the press, `drag-tick`
/// while it moves and `drag-end` on release/cancel. The background layer is
/// declared before the panel content, so rows and the resize handle stay on
/// top and keep their own clicks.
#[test]
fn a_left_drag_on_the_panel_moves_the_window() -> TestResult {
    let panel = include_str!("../ui/components/side-panel.slint").replace("\r\n", "\n");
    let app = include_str!("../ui/app.slint").replace("\r\n", "\n");

    let background = panel
        .split("panel-bg-hit := TouchArea {")
        .nth(1)
        .ok_or("the panel has a background interaction layer")?;
    let background = background
        .split("inner := Rectangle {")
        .next()
        .ok_or("the background layer ends at the panel content")?;

    assert!(
        background.contains("e.button == PointerEventButton.left"),
        "the panel background reacts to the left button"
    );
    assert!(
        background.contains("kind == \"down\"") && background.contains("root.drag-begin();"),
        "a left press starts the window move"
    );
    assert!(
        background.contains("kind == \"move\"")
            && background.contains("root.window-dragging")
            && background.contains("root.drag-tick();"),
        "moves drive the window move while the drag is in flight"
    );
    assert!(
        background.contains("kind == \"up\" || kind == \"cancel\"")
            && background.contains("root.drag-end();"),
        "release and cancel end the window move"
    );
    assert!(
        background.contains("PointerEventButton.right")
            && background.contains("root.right-click();"),
        "a right-click on empty panel space closes the rail"
    );
    assert!(
        app.contains("right-click => {") && app.contains("root.toggle-panel();"),
        "the side panel's right-click reaches the one toggle-panel action"
    );

    // Z-order: the background layer is declared before the rows, so the rows win
    // the pointer; the resize handle keeps its own capture at the right edge.
    let background_at = panel
        .find("panel-bg-hit := TouchArea {")
        .unwrap_or(usize::MAX);
    let rows_at = panel
        .find("for row in root.rows : ProjectRow {")
        .unwrap_or(0);
    assert!(
        background_at < rows_at,
        "the background layer sits under the rows"
    );
    assert!(
        panel.contains("mouse-cursor: col-resize;"),
        "the resize handle is still there"
    );

    // The app forwards the panel's three callbacks to the root move callbacks
    // (the same ones the canvas background and the home header use).
    let mount = app
        .split("SidePanel {")
        .nth(1)
        .ok_or("the editor mounts the side panel")?
        .split("canvas-area :=")
        .next()
        .ok_or("the side panel mount ends at the canvas")?;
    for forward in [
        "drag-begin => {\n                        root.drag-begin();",
        "drag-tick => {\n                        root.drag-tick();",
        "drag-end => {\n                        root.drag-end();",
    ] {
        assert!(
            mount.contains(forward),
            "the panel mount forwards {forward}"
        );
    }
    // The canvas mapping: left-drag moves the window, middle/right pan.
    assert!(
        app.contains("canvas-area.window-dragging = true;")
            && app.contains("e.button == PointerEventButton.left")
            && app.contains("root.drag-begin();")
            && app.contains("root.drag-tick();")
            && app.contains("root.drag-end();"),
        "the canvas left-drag window mapping is wired"
    );
    Ok(())
}

/// Compile-level removal guard: the deleted speech feature leaves no symbol in
/// the app's Rust or Slint sources.
#[test]
fn speech_feature_is_gone() {
    for source in [
        include_str!("image/ai.rs"),
        include_str!("ai_service.rs"),
        include_str!("node_ui.rs"),
        include_str!("edit_events.rs"),
        include_str!("events.rs"),
        include_str!("../ui/app.slint"),
        &include_str!("../ui/components/library.slint").replace("\r\n", "\n"),
        include_str!("../ui/components/image-ai-dialogs.slint"),
    ] {
        for needle in [
            "SpeechTool",
            "SpeechModal",
            "speech_open",
            "speech_close",
            "speech_field",
            "speech_pick",
            "speech_run",
            "speech_cancel",
            "speech-open",
            "speech-dialog",
            "speech-close",
            "speech-field",
            "speech-pick",
            "speech-run",
            "speech-cancel",
            "SPEECH_FORMAT_CHOICES",
            "SpeechDone",
            "SpeechError",
        ] {
            assert!(
                !source.contains(needle),
                "the speech feature is gone, found {needle}"
            );
        }
    }
}

/// The vertical ruler labels are **pre-rotated bitmaps**: Rust rasterises the
/// string at the exact pixel size, rotates the bitmap -90° and caches one
/// `slint::Image` per value, so the strip draws an upright Image and Slint can
/// never resample the glyphs. The `.slint` side therefore has no rotated Text.
#[test]
fn ruler_labels_are_pre_rotated_bitmaps() {
    let src = include_str!("../ui/components/rulers.slint");
    assert!(
        !src.contains("transform-rotation"),
        "a rotated Text would be resampled on every fractional pan"
    );
    assert!(src.contains("in property <[RulerTickImage]> labels-y: [];"));
    assert!(
        src.contains("for tick in root.labels-y : RulerLabelImage"),
        "the left strip renders the raster labels"
    );
    assert!(
        src.contains("source: root.tick.image;"),
        "the bitmap is the label"
    );
    assert!(
        src.contains("y: root.tick.label-y;"),
        "Rust computes a whole-pixel vertical placement"
    );
    assert!(
        src.contains("x: Math.round((26px - root.tick.label-width) / 2px) * 1px;"),
        "the bitmap is centred on the strip at a whole pixel"
    );
    // The rasteriser and its cache live in Rust, tested at the pixel level.
    let rust = include_str!("ruler_text.rs");
    assert!(rust.contains("ab_glyph"), "a small text rasteriser is used");
    assert!(
        rust.contains("thread_local!"),
        "one cached Image per distinct label"
    );
    assert!(
        rust.contains("pub fn place_label"),
        "whole-pixel placement is pinned by tests"
    );
    assert!(src.contains("out property <length> ruler-left-width: 26px;"));
    assert!(
        src.contains("x: root.ruler-left-width;"),
        "the top strip starts right of the left strip"
    );
}

/// The generation card body renders the model and every parameter flat, with
/// no second-level popover: the inline panel owns quality/size/res/count for
/// images, resolution/seconds/audio/watermark for video and reasoning/count
/// for text.
#[test]
fn generation_card_renders_every_parameter_inline() {
    let card = include_str!("../ui/components/node-card.slint");
    assert!(
        card.contains("InlineGenerationPanel {"),
        "the card mounts the inline panel"
    );
    assert!(card.contains("mode: root.mode;"));
    assert!(
        card.contains("mode: \"text\";"),
        "text generation shares the body"
    );
    let panel = include_str!("../ui/components/panel-popovers.slint");
    for label in [
        "\"Quality\"",
        "\"Size\"",
        "\"Res\"",
        "\"Count\"",
        "\"Seconds\"",
        "\"Audio\"",
        "\"Mark\"",
        "\"Reason\"",
    ] {
        assert!(panel.contains(label), "inline parameter row {label}");
    }
    assert!(
        !card.contains("ModelListCard {") && !card.contains("GenerationSettingsCard {"),
        "no second-level generation popover is mounted"
    );
}

/// The dock bar is a flat surface: the 1px hairline border and the glass panel
/// tier, and no outer glow. `drop-shadow-*` (Slint's outer shadow) must not
/// appear on the bar, and nothing may reintroduce the shadow token there.
#[test]
fn the_dock_bar_has_no_outer_glow() {
    let src = include_str!("../ui/components/toolbar.slint");
    let bar = src
        .split("export component DockBar inherits Rectangle {")
        .nth(1)
        .unwrap_or("");
    assert!(!bar.is_empty(), "the DockBar component exists");

    assert!(
        !src.contains("drop-shadow"),
        "the dock bar carries no outer drop shadow"
    );
    assert!(
        !src.contains("glass-panel-shadow"),
        "the dock bar does not reference the panel shadow token"
    );
    // Still a bar: the hairline edge and the panel tier survive.
    assert!(
        bar.contains("border-width: 1px;")
            && bar.contains("border-color: Theme.glass-panel-border;"),
        "the 1px hairline stays"
    );
    assert!(
        bar.contains("background: Theme.glass-panel;"),
        "the panel tier stays so the bar reads over the canvas"
    );
}

/// The chrome-wide shadow sweep: Slint's outer shadow is gone from every surface
/// in `ui/`, and the retired shadow tokens are gone with it. Separation comes
/// from the 1px hairline borders and the tier fills, never from a glow — a new
/// `drop-shadow-*` or a shadow token fails here.
///
/// The one legitimate literal is the Image Studio's layer-effect id
/// (`"drop-" + "shadow"`, Rust `Style::DropShadow`), which is content, not
/// chrome; assembling it from two halves keeps this scan exact.
#[test]
fn the_ui_has_no_outer_shadow() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::{Path, PathBuf};

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|ext| ext == "slint") {
                out.push(path);
            }
        }
        Ok(())
    }

    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files = Vec::new();
    walk(&ui, &mut files)?;
    assert!(
        files.len() > 30,
        "the ui tree was walked ({} files)",
        files.len()
    );
    for path in files {
        let src = std::fs::read_to_string(&path)?;
        for needle in [
            "drop-shadow-color",
            "drop-shadow-blur",
            "drop-shadow-offset",
            "drop-shadow",
            "glass-panel-shadow",
            "picker-handle-shadow",
        ] {
            assert!(
                !src.contains(needle),
                "{} still declares an outer shadow ({needle})",
                path.display()
            );
        }
    }
    Ok(())
}

/// The dock bar exposes exactly its remaining actions, in order, with one
/// rhythm: 4px inside a group, 6px around each divider, a 10px outer gutter,
/// one centre line and no extra separator line. The node list, the canvas
/// background menu, the zoom read-out and the plugin entries are gone; node
/// creation is the centred group of direct node-type buttons, and the
/// side-panel toggle + Home lead the bar from the left.
#[expect(
    clippy::too_many_lines,
    reason = "one source-scan assertion list for the dock's declared action order"
)]
#[test]
fn dock_bar_groups_its_actions_in_order() {
    let src = include_str!("../ui/components/toolbar.slint");
    let bar = src
        .split("export component DockBar inherits Rectangle {")
        .nth(1)
        .unwrap_or("");
    assert!(!bar.is_empty(), "the DockBar component exists");

    // Model-level action order: every `root.action(...)` the bar emits, in
    // source order (the one-button-per-action contract).
    let mut actions: Vec<String> = Vec::new();
    for line in bar.lines() {
        let mut rest = line;
        while let Some(pos) = rest.find("root.") {
            let tail = &rest[pos + 5..];
            if tail.is_empty() {
                break;
            }
            let name_len = tail
                .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                .unwrap_or(tail.len());
            if name_len == 0 {
                rest = tail.get(1..).unwrap_or("");
                continue;
            }
            let name = &tail[..name_len];
            if tail[name_len..].starts_with('(') {
                actions.push(name.to_string());
            }
            rest = &tail[name_len..];
        }
    }
    let expected = [
        "toggle-panel",
        "undo",
        "redo",
        "create-node",
        "delete-selection",
    ];
    assert_eq!(actions, expected, "the dock's action order");

    // Three groups: the nav/history seam, the history/node seam and the
    // conditional delete seam (an `if` divider, absent until a selection).
    assert_eq!(
        src.matches("DockDivider {").count(),
        3,
        "two fixed dividers and the one conditional delete-seam divider"
    );
    // Root row + three button groups all share one centre line.
    assert_eq!(
        bar.matches("cross-axis-alignment: center;").count(),
        4,
        "every row and group is vertically centred"
    );
    assert!(src.contains("padding-left: 10px;") && src.contains("padding-right: 10px;"));
    assert_eq!(
        bar.matches("spacing: 6px;").count(),
        1,
        "one 6px rhythm: the gaps around every divider"
    );
    assert_eq!(
        bar.matches("spacing: 4px;").count(),
        3,
        "one gutter inside every button group"
    );
    // The bar hugs its content: no equal-width flanking groups, no stretch
    // placeholders and no spacer rectangles. The whole bar is centred, which is
    // what keeps the node group visually central.
    assert!(
        !src.contains("side-width") && !src.contains("left-group") && !src.contains("right-group"),
        "the flanking groups are gone"
    );
    assert!(
        !bar.contains("horizontal-stretch"),
        "nothing in the dock stretches"
    );
    assert!(
        !bar.contains("alignment: start") && !bar.contains("alignment: end"),
        "no group is start/end aligned inside a stretched slot"
    );
    assert!(
        src.contains("width: self.preferred-width;"),
        "the bar hugs its content"
    );
    // Blank-area guard: no layout child is merely `visible: false`, which would
    // still reserve its slot (the old delete seam did exactly that). The
    // conditional actions use `if`, so they leave the layout entirely.
    assert!(
        !bar.contains("visible:"),
        "no hidden-but-space-taking child may reserve a slot inside the bar"
    );
    assert!(
        bar.contains("if root.has-selection : DockDivider {")
            && bar.contains("if root.has-selection : IconButton {"),
        "the delete seam is conditional, not hidden-in-place"
    );
    assert!(
        src.contains("for type-id in root.create-types"),
        "one direct button per creatable node type"
    );
    assert!(
        !src.contains("\"text\""),
        "the dock no longer offers a Text node button"
    );
    // The five removed dock features leave no symbol behind.
    for gone in [
        "plugin-types-button",
        "Icons.plugin-types",
        "Icons.puzzle",
        "Icons.list-tree",
        "Icons.grid",
        "zoom-label",
        "toggle-zoom",
        "toggle-node-list",
        "toggle-background",
        "toggle-plugin-types",
        "open-plugins",
        "BackgroundPopover",
        "ZoomPanel",
    ] {
        assert!(!src.contains(gone), "the dock no longer carries {gone}");
    }
    // Every dock button carries a two-stage tooltip: a name plus one line.
    assert_eq!(
        bar.matches("tip-description:").count(),
        5,
        "every dock button (the node-type loop included) names itself and explains what it does"
    );
    assert!(
        !bar.contains("Layered glass edges") && !bar.contains("y: parent.height - 1px;"),
        "the bar's own border is the only edge line"
    );
    // The trash is the last conditional action and stays conditional: its `if`
    // is declared before the button. There is no trailing fullscreen button.
    let trash_if = bar.rfind("if root.has-selection : IconButton").unwrap_or(0);
    let trash = bar.rfind("Icons.trash").unwrap_or(0);
    let node_group = bar.rfind("Icons.kind-icon").unwrap_or(0);
    assert!(
        trash > node_group,
        "the delete group follows the node group"
    );
    assert!(
        trash_if < trash,
        "the trash's conditional seam wraps its own button"
    );
    assert!(
        !src.contains("toggle-fullscreen") && !src.contains("root.fullscreen"),
        "the fullscreen toggle and its state are gone"
    );
    // The panel toggle leads the bar; Home moved to the side-panel header.
    assert!(
        src.contains("icon: Icons.panel-left;") && !src.contains("icon: Icons.home;"),
        "the panel toggle leads the bar and Home lives in the side panel"
    );
    assert!(
        bar.find("root.toggle-panel").unwrap_or(usize::MAX) < bar.find("root.undo").unwrap_or(0),
        "the nav group comes before history"
    );
    assert!(
        !src.contains("root.toggle-tool")
            && !src.contains("tool-pan")
            && !src.contains("Icons.pointer")
            && !src.contains("Icons.move"),
        "the select/pan tool toggle is gone from the dock"
    );
}

/// Removed dock features leave no UI or Rust symbol behind: import/export, the
/// minimap and the dock's own side-panel toggle.
#[test]
fn removed_dock_features_are_gone() {
    for (name, source) in [
        (
            "toolbar.slint",
            include_str!("../ui/components/toolbar.slint"),
        ),
        ("app.slint", include_str!("../ui/app.slint")),
        ("panels.rs", include_str!("../../app-core/src/panels.rs")),
        ("vm.rs", include_str!("vm.rs")),
        ("app.rs", include_str!("app.rs")),
        ("events.rs", include_str!("events.rs")),
        ("handlers.rs", include_str!("handlers.rs")),
        ("shortcuts.rs", include_str!("shortcuts.rs")),
        ("theme.slint", include_str!("../ui/theme.slint")),
    ] {
        for needle in [
            "MiniMap",
            "MinimapBlob",
            "minimap_open",
            "minimap-open",
            "toggle_minimap",
            "toggle-minimap",
            "minimap_pointer",
            "minimap-pointer",
            "import-media",
            "export-project",
            "media-import",
            "media_import",
        ] {
            assert!(!source.contains(needle), "{name} still references {needle}");
        }
    }
    // The dock carries the canvas side-panel toggle; the canvas itself has no
    // chrome of its own any more.
    let dock = include_str!("../ui/components/toolbar.slint");
    assert!(
        dock.contains("Icons.panel-left"),
        "the dock carries the panel toggle"
    );
    assert!(
        dock.contains("callback toggle-panel();"),
        "the dock emits the panel callback"
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("ui/components/top-bar.slint")
            .exists(),
        "the old title bar source is gone"
    );
}

/// The home screen layout: ONE flat surface, two columns under a single app
/// band — PROJECTS at the top left with the settings gear and window controls at
/// the top right (the ghost New Project `+` sits beside PROJECTS), then the
/// ~240px project sidebar (one row per project) and the right pane (a 26px
/// project header with its ghost New Canvas `+`, the 18px table column header,
/// then the full-width canvas table). No folder chip strip, no split card plate.
#[test]
fn the_home_screen_is_one_flat_surface() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    for retired in ["260px", "glass-panel", "Theme.card", "library-eyebrow"] {
        assert!(
            !home.contains(retired),
            "the flat home screen keeps no {retired} token"
        );
    }
    assert!(
        home.contains("background: transparent;"),
        "the screen adds no plate of its own: it draws on the one window surface"
    );
    assert!(
        home.contains("component ProjectSidebarRow inherits Rectangle")
            && home.contains("component CanvasRow inherits Rectangle"),
        "projects are sidebar rows and canvases are table rows"
    );
    assert!(
        home.contains("width: 240px;")
            && home.contains("icon: Icons.home;")
            && !home.contains("I18n.library-projects-label"),
        "the project sidebar is the 240px column under the Home button, not a PROJECTS wordmark"
    );
    assert!(
        home.contains("tip: I18n.library-new-project;"),
        "the app band offers New Project"
    );
    assert!(
        home.contains("tip: I18n.library-new-canvas;"),
        "the right pane header offers New Canvas"
    );

    // The app band: the drag surface, Home, New Project, then the gear and the
    // window controls at the far right.
    let drag_at = home.find("WindowDragArea {").unwrap_or(usize::MAX);
    let home_at = home.find("icon: Icons.home;").unwrap_or(usize::MAX);
    let new_project_at = home
        .find("tip: I18n.library-new-project;")
        .unwrap_or(usize::MAX);
    let gear_at = home.find("icon: Icons.settings;").unwrap_or(usize::MAX);
    let controls_at = home.find("WindowButtons {").unwrap_or(usize::MAX);
    let new_canvas_at = home.find("tip: I18n.library-new-canvas;").unwrap_or(0);
    assert!(
        drag_at < home_at
            && home_at < new_project_at
            && new_project_at < gear_at
            && gear_at < controls_at,
        "Home and New Project sit in the top band before the chrome controls"
    );
    assert!(
        controls_at < new_canvas_at,
        "the window drag/controls live above the project header row"
    );
    assert!(
        home.contains("property <length> title-band: 76px;")
            && home.contains("property <length> header-band: 44px;")
            && home.contains("property <length> project-header-band: 26px;")
            && home.matches("height: root.header-band;").count() == 0
            && home.contains("height: root.header-band - root.project-header-band;"),
        "a 76px app band sits above a 44px shared header stack (26px project row + 18px column header)"
    );
    assert!(
        home.contains("text: I18n.library-name-column;")
            && home.contains("text: I18n.library-updated-column;")
            && home.contains("text: I18n.library-actions-column;"),
        "the table header names the three centred columns"
    );
    assert!(
        !home.contains("group-count") && !home.contains("library-group-count"),
        "the home screen carries no count badge"
    );
    assert!(
        !home.contains("Icons.moon") && !home.contains("toggle-theme"),
        "the theme control is not in the chrome any more"
    );
    assert!(
        home.contains("I18n.library-empty-no-projects")
            && home.contains("I18n.library-empty-project"),
        "both empty states exist (no project, empty project)"
    );
}

/// The home header is the window drag surface now that no title bar exists; it
/// feeds the same begin/tick/end path the canvas left-drag and the studio use.
#[test]
fn the_home_header_drags_the_window_from_the_app_loop() {
    let home = include_str!("../ui/components/library.slint").replace("\r\n", "\n");
    assert!(
        !home.contains("WindowMoveArea"),
        "the native modal move loop is gone"
    );
    assert!(
        home.contains("callback drag-begin();")
            && home.contains("callback drag-tick();")
            && home.contains("callback drag-end();"),
        "the header reports the three drag phases"
    );
    assert!(
        home.contains("root.drag-begin();")
            && home.contains("root.drag-tick();")
            && home.contains("root.drag-end();"),
        "the header TouchArea drives all three phases"
    );
    assert!(
        home.contains("e.kind == PointerEventKind.down") && home.contains("self.pressed"),
        "the move only follows a held left button"
    );
    assert!(
        home.contains("double-clicked =>"),
        "double-click still maximises"
    );

    let app = include_str!("../ui/app.slint").replace("\r\n", "\n");
    for forwarded in ["drag-begin => {", "drag-tick => {", "drag-end => {"] {
        assert!(app.contains(forwarded), "the root forwards {forwarded}");
    }

    let events = include_str!("events.rs");
    assert!(
        events.contains("ui.on_drag_begin") && events.contains("app.drag_begin(&ui)"),
        "drag-begin reaches the app"
    );
    assert!(
        events.contains("ui.on_drag_tick") && events.contains("app.drag_tick(&ui)"),
        "drag-tick reaches the app"
    );
    assert!(
        events.contains("ui.on_drag_end") && events.contains("App::drag_end"),
        "drag-end reaches the app"
    );

    let glass = include_str!("../../app-core/src/glass.rs");
    assert!(
        glass.contains("fn GetCursorPos(point: *mut WinPoint) -> i32;"),
        "the Win32 cursor read is declared next to the DWM calls"
    );
    assert!(
        glass.contains("pub fn cursor_pos()"),
        "the cursor read is exposed as a helper"
    );

    let app_rs = include_str!("app.rs");
    assert!(
        app_rs.contains("drag_target(drag.window, drag.cursor, cursor)"),
        "a tick computes the target from the cursor delta"
    );
    assert!(
        app_rs.contains("set_position(slint::PhysicalPosition::new(position.0, position.1))"),
        "a tick positions the window"
    );
}

/// There is no application title bar in any view: the home screen owns the
/// window controls, the canvas keeps the left-drag move and the studio keeps
/// its own menu row.
#[test]
fn no_view_mounts_an_app_title_bar() {
    let app = include_str!("../ui/app.slint").replace("\r\n", "\n");
    assert!(
        !app.contains("TopBar"),
        "the top bar is not mounted anywhere"
    );
    assert!(
        !app.contains("title-editing") && !app.contains("title-edit-begin"),
        "the top-bar inline project rename is gone"
    );
    assert!(
        !app.contains("workspace-tab") && !app.contains("select-workspace"),
        "the workspace tab strip is gone"
    );
    assert!(
        app.contains("canvas-area.window-dragging = true;")
            && app.contains("root.drag-begin();")
            && app.contains("root.drag-tick();")
            && app.contains("root.drag-end();"),
        "a left-drag on the canvas background still moves the window"
    );
    let studio = include_str!("../ui/components/image.slint");
    assert!(
        studio.contains("callback drag-begin();"),
        "the studio keeps its own drag affordance"
    );
}
