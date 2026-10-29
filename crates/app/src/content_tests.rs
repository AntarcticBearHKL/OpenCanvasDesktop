//! Content-layer state tests: the retry/reset transition, editor commits into
//! the document, chip deletion and the background-mode switch.

use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeStatus};
use slint::ComponentHandle;
use slint::Model;

use crate::app::App;
use crate::inline_edit::EditTarget;
use crate::store::Store;

fn app_with_demo() -> App {
    let mut app = App::new(Store::at(std::env::temp_dir()));
    app.doc = crate::doc::Doc::demo();
    app
}

fn set_status(app: &mut App, id: &str, status: CanvasNodeStatus, error: Option<&str>) {
    if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.status = Some(status);
        meta.error_details = error.map(str::to_string);
    }
}

fn status_of(app: &App, id: &str) -> Option<CanvasNodeStatus> {
    app.doc
        .node(id)
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.status)
}

#[test]
fn retry_resets_a_stale_error_exactly_once() {
    let mut app = app_with_demo();
    assert!(!app.doc.can_undo(), "a fresh document has no history");
    let pristine = status_of(&app, "image");
    set_status(
        &mut app,
        "image",
        CanvasNodeStatus::Error,
        Some("The request failed"),
    );
    let seeded = app.now_ms() + 1_000;
    app.doc.commit(app.now_ms());
    app.doc.tick_history(seeded);
    assert!(app.doc.can_undo(), "the failure is the committed document");

    assert!(app.reset_node_error("image"), "the error clears");
    assert_eq!(status_of(&app, "image"), Some(CanvasNodeStatus::Idle));
    let error = app
        .doc
        .node("image")
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.error_details.clone());
    assert_eq!(error, None, "the error detail is dropped");

    // The reset sits on top of the failure: one tick, then two undos walk the
    // history back (failure state, then the pristine document).
    let now = app.now_ms() + 1_000;
    app.doc.tick_history(now);
    assert!(app.doc.undo(), "the reset is one history entry");
    assert_eq!(
        status_of(&app, "image"),
        Some(CanvasNodeStatus::Error),
        "undo restores the failure state"
    );
    assert!(app.doc.undo(), "and the seeding commit sits below it");
    assert_eq!(
        status_of(&app, "image"),
        pristine,
        "back to the demo document"
    );
    assert!(!app.doc.can_undo(), "the history is fully walked back");

    assert!(!app.reset_node_error("image"), "a second retry is a no-op");
    assert!(!app.reset_node_error("missing-node"));
}

#[test]
fn body_edit_commit_writes_the_document_once() -> Result<(), &'static str> {
    let mut app = app_with_demo();
    assert!(app
        .edit
        .begin_body("text", "The quick brown fox\n快捷的狐狸"));
    assert!(app.edit.change("text", "edited body"));
    let committed = app.edit.commit().ok_or("the dirty draft must commit")?;
    assert_eq!(committed.target, EditTarget::Body);
    assert!(app.apply_edit_to_doc(&committed));
    let content = app
        .doc
        .node("text")
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.content.clone());
    assert_eq!(content.as_deref(), Some("edited body"));
    let now = app.now_ms() + 1_000;
    app.doc.tick_history(now);
    assert!(app.doc.can_undo(), "one history entry for the commit");

    // An unchanged draft owns no document write.
    assert!(app.edit.begin_body("text", "edited body"));
    assert!(app.edit.commit().is_none());
    Ok(())
}

#[test]
fn title_edit_commit_renames_the_node() -> Result<(), &'static str> {
    let mut app = app_with_demo();
    assert!(app.edit.begin_title("image", "Image"));
    assert!(app.edit.change("image", "  Cover  "));
    let committed = app.edit.commit().ok_or("the trimmed title must commit")?;
    assert!(app.apply_edit_to_doc(&committed));
    assert_eq!(
        app.doc.node("image").map(|node| node.title.clone()),
        Some("Cover".to_string())
    );
    Ok(())
}

#[test]
fn cancel_leaves_the_document_untouched() {
    let mut app = app_with_demo();
    let before = app
        .doc
        .node("text")
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.content.clone());
    assert!(app.edit.begin_body("text", "original"));
    assert!(app.edit.change("text", "thrown away"));
    assert!(app.edit.cancel().is_some());
    assert!(app.edit.commit().is_none(), "no commit after a cancel");
    let after = app
        .doc
        .node("text")
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.content.clone());
    assert_eq!(before, after, "Escape never writes");
    assert!(!app.doc.can_undo());
}

#[test]
fn chip_delete_is_scoped_to_the_reference_and_blocks_locked_nodes() {
    let mut app = app_with_demo();
    if let Some(node) = app
        .doc
        .nodes_mut()
        .iter_mut()
        .find(|node| node.id == "prompt")
    {
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.references = Some(vec!["image".into(), "video-prompt".into()]);
    }
    assert!(app.remove_chip_from_doc("prompt", "image"));
    let left = app
        .doc
        .node("prompt")
        .and_then(|node| node.metadata.as_ref())
        .and_then(|meta| meta.references.clone());
    assert_eq!(left, Some(vec!["video-prompt".to_string()]));

    assert!(
        !app.remove_chip_from_doc("prompt", "image"),
        "already removed"
    );
    if let Some(node) = app
        .doc
        .nodes_mut()
        .iter_mut()
        .find(|node| node.id == "prompt")
    {
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.locked = Some(true);
    }
    assert!(
        !app.remove_chip_from_doc("prompt", "video-prompt"),
        "a locked node's chips stay"
    );
}

/// A file row crossing the press threshold creates a real Image node at once:
/// the node, not a chip, is the preview. Its loading plate stays until the
/// worker reports the decoded bitmap, and moves go through the coalesced node
/// drag (one tick, not one write per pointer event).
#[test]
fn assets_drag_start_creates_a_real_node_and_moves_it() -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;
    use slint::Model;

    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let folder = tempfile::tempdir()?;
    let file = folder.path().join("hero.png");
    std::fs::write(&file, b"not really a png")?;

    crate::app::with_app(&app, |app| {
        app.doc = crate::doc::Doc::demo();
        let id = app.doc.create_node(
            CanvasNodeType::Assets,
            crate::coords::WorldPos::new(0.0, 0.0),
            app.now_ms(),
        );
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .and_then(|node| node.metadata.as_mut())
        {
            meta.asset_folder = Some(folder.path().to_string_lossy().to_string());
        }
        app.sync_all(&ui);
        let rows_before = ui.get_nodes().row_count();
        let nodes_before: Vec<String> =
            app.doc.nodes().iter().map(|node| node.id.clone()).collect();
        // The panel is closed here so window and canvas coordinates coincide;
        // the open-panel conversion has its own test below.
        ui.set_side_panel_open(false);
        app.asset_drag_begin(&ui, &id, "hero.png", 120.0, 80.0);
        assert_eq!(
            app.doc.nodes().len(),
            nodes_before.len() + 1,
            "a real node exists immediately"
        );
        assert_eq!(
            ui.get_nodes().row_count(),
            rows_before + 1,
            "the new card is in the canvas model at once"
        );
        let created = app
            .doc
            .nodes()
            .iter()
            .find(|node| !nodes_before.contains(&node.id))
            .expect("the temporary image node");
        assert_eq!(created.node_type, CanvasNodeType::Image);
        assert_eq!(created.title, "hero");
        assert!(
            app.doc.selection().contains(&created.id),
            "the preview node is selected"
        );
        assert!(
            created.metadata.as_ref().and_then(|meta| meta.status)
                == Some(open_canvas_core::node::CanvasNodeStatus::Loading),
            "the loading plate shows until the decode lands"
        );
        let start = created.position;
        let id_before = created.id.clone();
        assert!(
            matches!(app.doc.drag(), Some(crate::doc::Drag::Nodes { .. })),
            "the preview is moved by the ordinary node-drag gesture"
        );

        // A move only records the pointer; the frame tick applies it once.
        app.asset_drag_moved(&ui, 200.0, 140.0);
        let unchanged = app.doc.node(&id_before).expect("the node").position;
        assert_eq!(unchanged, start, "no document write before the tick");
        app.node_drag_tick(&ui);
        let moved = app.doc.node(&id_before).expect("the node").position;
        assert_ne!(moved, start, "the tick moves the real node");
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Assets row drag: the press guard and the release path
// ---------------------------------------------------------------------------

/// The row itself, at a known position: pointer events can be dispatched at
/// exact coordinates and every drag callback is counted (the log proves the
/// release reports `drag-out` before `drag-end`).
mod row_probe {
    slint::slint! {
        import { AssetTreeRowView } from "../ui/components/node-card.slint";
        import { AssetTreeRow } from "../ui/components/overlay-types.slint";

        export component AssetRowProbe inherits Window {
            in property <AssetTreeRow> file-row: { path: "hero.png", label: "hero.png", depth: 0, kind: "image", expanded: false };
            in-out property <int> activates: 0;
            in-out property <int> starts: 0;
            in-out property <int> moves: 0;
            in-out property <int> outs: 0;
            in-out property <int> ends: 0;
            in-out property <string> log: "";
            width: 240px;
            height: 32px;
            background: #ffffff;

            AssetTreeRowView {
                x: 0px;
                y: 0px;
                width: 240px;
                height: 26px;
                entry: root.file-row;
                vs: 1;
                activate => {
                    root.activates += 1;
                }
                drag-start(x, y) => {
                    root.starts += 1;
                    root.log += "start;";
                }
                drag-move(x, y) => {
                    root.moves += 1;
                }
                drag-out => {
                    root.outs += 1;
                    root.log += "out;";
                }
                drag-end => {
                    root.ends += 1;
                    root.log += "end;";
                }
            }
        }
    }
}

fn init_test_backend() {
    // A testing platform is installed once per thread; a second install is a
    // no-op (the first one is still the one in use).
    if crate::config_test_support::init_backend().is_err() {
        // A platform is already installed on this thread.
    }
}

fn press(ui: &row_probe::AssetRowProbe, x: f32, y: f32) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        });
}

fn hover_to(ui: &row_probe::AssetRowProbe, x: f32, y: f32) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
}

fn release(ui: &row_probe::AssetRowProbe, x: f32, y: f32) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        });
}

/// A cancelled drag (the scrollable tree took the pointer over) stays in
/// flight: the row must not clear it, because the canvas release resolves the
/// drop from the pointer-up position.
#[test]
fn a_cancelled_drag_stays_pending_for_the_canvas_release() -> Result<(), Box<dyn std::error::Error>>
{
    init_test_backend();
    let ui = row_probe::AssetRowProbe::new()?;
    ui.show()?;
    press(&ui, 20.0, 12.0);
    hover_to(&ui, 80.0, 12.0);
    assert_eq!(ui.get_starts(), 1);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerExited);
    assert_eq!(ui.get_outs(), 0, "a cancel is not a drop");
    assert_eq!(ui.get_ends(), 0, "a cancel keeps the drag pending");
    assert_eq!(ui.get_log().as_str(), "start;");
    Ok(())
}

/// A pointer moving across a file row without a button held is a hover: it
/// never arms the drag, so no ghost can pop up under the cursor.
#[test]
fn hovering_a_file_row_never_starts_a_drag() -> Result<(), Box<dyn std::error::Error>> {
    init_test_backend();
    let ui = row_probe::AssetRowProbe::new()?;
    ui.show()?;
    hover_to(&ui, 20.0, 12.0);
    hover_to(&ui, 120.0, 12.0);
    hover_to(&ui, 220.0, 12.0);
    assert_eq!(ui.get_starts(), 0, "a hover must not report a drag start");
    assert_eq!(ui.get_outs(), 0, "a hover must not report a drop");
    assert_eq!(ui.get_log().as_str(), "");
    Ok(())
}

/// A held pointer past the 4px threshold reports exactly one drag start; each
/// further move only moves it. On release the drop is reported before the
/// drag ends, so the drop can read the in-flight drag state.
#[test]
fn a_pressed_move_starts_one_drag_and_drops_before_ending() -> Result<(), Box<dyn std::error::Error>>
{
    init_test_backend();
    let ui = row_probe::AssetRowProbe::new()?;
    ui.show()?;
    press(&ui, 20.0, 12.0);
    // Under the threshold: still a click.
    hover_to(&ui, 23.0, 12.0);
    assert_eq!(ui.get_starts(), 0, "a short move stays a click");
    // Past the threshold: one drag starts.
    hover_to(&ui, 60.0, 12.0);
    assert_eq!(ui.get_starts(), 1, "the drag starts once");
    assert_eq!(ui.get_moves(), 0, "the starting move is not also a move");
    // Later moves only move the ghost.
    hover_to(&ui, 90.0, 12.0);
    hover_to(&ui, 150.0, 12.0);
    assert_eq!(ui.get_starts(), 1, "no second drag start");
    assert_eq!(ui.get_moves(), 2, "the following moves are reported");

    release(&ui, 150.0, 12.0);
    assert_eq!(ui.get_outs(), 1, "the release reports a drop");
    assert_eq!(ui.get_ends(), 1, "the drag ends exactly once");
    assert_eq!(
        ui.get_log().as_str(),
        "start;out;end;",
        "the drop is processed before the drag state is cleared"
    );
    assert_eq!(ui.get_activates(), 0, "a file row never activates");
    Ok(())
}

/// Hover after a drag is over must not restart one (the release clears the
/// state and the next hover move stays inert).
#[test]
fn a_move_after_the_release_does_not_restart_the_drag() -> Result<(), Box<dyn std::error::Error>> {
    init_test_backend();
    let ui = row_probe::AssetRowProbe::new()?;
    ui.show()?;
    press(&ui, 20.0, 12.0);
    hover_to(&ui, 80.0, 12.0);
    release(&ui, 80.0, 12.0);
    hover_to(&ui, 200.0, 12.0);
    assert_eq!(ui.get_starts(), 1, "the post-release hover stays inert");
    assert_eq!(ui.get_outs(), 1);
    Ok(())
}

/// An app with a demo canvas, one bound assets node and one image file.
struct AssetsFixture {
    ui: crate::AppWindow,
    app: std::rc::Rc<std::cell::RefCell<App>>,
    _store_dir: tempfile::TempDir,
    _folder: tempfile::TempDir,
    assets_id: String,
    file_name: String,
    bytes: Vec<u8>,
}

fn assets_fixture() -> Result<AssetsFixture, Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    init_test_backend();
    let (store_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let folder = tempfile::tempdir()?;
    let bytes = b"assets drag payload".to_vec();
    let file_name = "hero.png".to_string();
    std::fs::write(folder.path().join(&file_name), &bytes)?;
    let assets_id = crate::app::with_app_r(&app, |app| {
        // A clean canvas (only the assets card): the drop tests need genuinely
        // empty space to assert that a card lands exactly on the release point.
        app.doc = crate::doc::Doc::assemble(Vec::new(), Vec::new());
        let id = app.doc.create_node(
            CanvasNodeType::Assets,
            crate::coords::WorldPos::new(1500.0, 1200.0),
            app.now_ms(),
        );
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .and_then(|node| node.metadata.as_mut())
        {
            meta.asset_folder = Some(folder.path().to_string_lossy().to_string());
        }
        app.sync_all(&ui);
        id
    })
    .ok_or("the app is busy")?;
    Ok(AssetsFixture {
        ui,
        app,
        _store_dir: store_dir,
        _folder: folder,
        assets_id,
        file_name,
        bytes,
    })
}

impl AssetsFixture {
    /// Canvas-local x of the canvas column (the side panel sits before it).
    fn canvas_origin(&self) -> f64 {
        if self.ui.get_side_panel_open() {
            f64::from(self.ui.get_side_panel_width())
        } else {
            0.0
        }
    }

    fn node_ids(&self) -> Vec<String> {
        crate::app::with_app_r(&self.app, |app| {
            app.doc.nodes().iter().map(|node| node.id.clone()).collect()
        })
        .unwrap_or_default()
    }

    fn node(&self, id: &str) -> Option<open_canvas_core::node::CanvasNodeData> {
        crate::app::with_app_r(&self.app, |app| app.doc.node(id).cloned()).flatten()
    }

    /// Pans/zooms the canvas on both sides of the boundary: the document (the
    /// drop's `screen_to_world`) and the window (what the canvas paints).
    fn set_viewport(&self, viewport: crate::coords::Viewport) {
        crate::app::with_app(&self.app, |app| {
            app.doc.viewport = viewport;
            app.sync_view(&self.ui);
        });
    }

    /// Side-panel state. `width` only matters while the panel is open; it is
    /// written to the window and the app so the drag path and the layout agree.
    fn set_panel(&self, open: bool, width: f64) {
        self.ui.set_side_panel_open(open);
        self.ui.set_side_panel_width(crate::vm::len(width));
        crate::app::with_app(&self.app, |app| {
            app.side_open = open;
            app.side_width = width;
        });
    }

    /// Begins, moves and releases the drag at `world`, through the same App
    /// entry points the row callbacks use. `last_canvas_pos` is deliberately
    /// stale: the drop must follow the drag, not the canvas' last hover.
    fn drag_and_drop(&self, world: crate::coords::WorldPos) {
        let screen =
            crate::app::with_app_r(&self.app, |app| app.doc.viewport().world_to_screen(world))
                .expect("the app is busy");
        let window = crate::coords::ScreenPos::new(screen.x + self.canvas_origin(), screen.y);
        crate::app::with_app(&self.app, |app| {
            app.last_canvas_pos = Some(crate::coords::ScreenPos::new(5.0, 5.0));
            app.asset_drag_begin(
                &self.ui,
                &self.assets_id,
                &self.file_name,
                window.x,
                window.y,
            );
            app.asset_drag_moved(&self.ui, window.x, window.y);
            app.asset_drag_file(&self.ui, &self.assets_id, &self.file_name);
        });
    }

    fn drag_and_drop_back_on_the_card(&self) {
        let card = self.node(&self.assets_id).expect("the assets node");
        let centre = crate::coords::WorldPos::new(
            card.position.x + card.width / 2.0,
            card.position.y + card.height / 2.0,
        );
        self.drag_and_drop(centre);
    }

    /// Arms the row drag at `world` and leaves the pointer down: the ghost is
    /// visible and the payload is in flight, but no release has happened. Returns
    /// the canvas-local screen point of the release position.
    fn begin_drag_to(&self, world: crate::coords::WorldPos) -> crate::coords::ScreenPos {
        let screen =
            crate::app::with_app_r(&self.app, |app| app.doc.viewport().world_to_screen(world))
                .expect("the app is busy");
        let window = crate::coords::ScreenPos::new(screen.x + self.canvas_origin(), screen.y);
        crate::app::with_app(&self.app, |app| {
            app.asset_drag_begin(
                &self.ui,
                &self.assets_id,
                &self.file_name,
                window.x,
                window.y,
            );
            app.asset_drag_moved(&self.ui, window.x, window.y);
        });
        screen
    }

    /// Releases the pointer over the canvas at `point`, the way the canvas
    /// background's `pointer-event` forwards it.
    fn canvas_up(&self, point: crate::coords::ScreenPos) {
        self.canvas_pointer(crate::input::PointerKind::Up, point);
    }

    fn canvas_pointer(&self, kind: crate::input::PointerKind, point: crate::coords::ScreenPos) {
        let event = crate::input::PointerEvent {
            kind,
            pos: point,
            button: crate::input::Button::Left,
            shift: false,
            ctrl: false,
        };
        crate::app::with_app(&self.app, |app| app.canvas_pointer(&self.ui, &event));
    }

    /// The real node the drag is moving, if a row drag is in flight.
    fn drag_node(&self) -> Option<open_canvas_core::node::CanvasNodeData> {
        let id = crate::app::with_app_r(&self.app, |app| {
            app.asset_drag.as_ref().map(|drag| drag.node_id.clone())
        })
        .flatten()?;
        self.node(&id)
    }

    fn drag_pending(&self) -> bool {
        crate::app::with_app_r(&self.app, |app| app.asset_drag.is_some()).unwrap_or(false)
    }

    fn pending_payloads(&self) -> usize {
        crate::app::with_app_r(&self.app, |app| app.pending_media.len()).unwrap_or(0)
    }
}

/// The file's bytes as the node content the drop stores.
fn dropped_content(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    format!("data:image/png;base64,{}", BASE64.encode(bytes))
}

/// One tiny real raster per format the assets tree accepts. The pixel pattern
/// is deterministic, so a decode test can prove the bytes survived the drag.
fn raster_fixture(format: image::ImageFormat) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut rgba = image::RgbaImage::new(4, 3);
    for (x, y, pixel) in rgba.enumerate_pixels_mut() {
        *pixel = image::Rgba([
            u8::try_from(x * 40).unwrap_or(0),
            u8::try_from(y * 60).unwrap_or(0),
            120,
            255,
        ]);
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba).write_to(&mut cursor, format)?;
    Ok(cursor.into_inner())
}

/// Releasing over empty canvas creates exactly one image node, centred on the
/// release point, carrying the dragged file's content, and the canvas layer is
/// rebuilt so the card is actually on screen.
#[test]
fn assets_drag_drop_on_empty_canvas_creates_the_image_at_the_release_point(
) -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;
    use slint::Model;

    let fixture = assets_fixture()?;
    let drop_world = crate::coords::WorldPos::new(1700.0, 900.0);
    let before = fixture.node_ids();
    fixture.drag_and_drop(drop_world);
    let after = fixture.node_ids();
    assert_eq!(after.len(), before.len() + 1, "one node is created");
    let created = after
        .iter()
        .find(|id| !before.contains(id))
        .expect("a new node");
    let node = fixture.node(created).expect("the created node exists");
    assert_eq!(node.node_type, CanvasNodeType::Image);
    assert_eq!(
        node.metadata.as_ref().and_then(|meta| meta.content.clone()),
        Some(dropped_content(&fixture.bytes)),
        "the node carries the dragged file's content"
    );
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - drop_world.x).abs() < 0.5 && (centre.1 - drop_world.y).abs() < 0.5,
        "the card is centred on the release point ({centre:?})"
    );
    crate::app::with_app(&fixture.app, |app| {
        assert!(app.asset_drag.is_none(), "the drag is consumed");
    });
    assert_eq!(
        fixture.ui.get_nodes().row_count(),
        after.len(),
        "the canvas model gained the new card"
    );
    Ok(())
}

/// The preview is the user's placement: releasing over another card leaves it
/// under the pointer (no post-drop nudge), exactly like moving an existing
/// node there.
#[test]
fn assets_drag_release_keeps_the_node_under_the_pointer() -> Result<(), Box<dyn std::error::Error>>
{
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let drop_world = crate::coords::WorldPos::new(1700.0, 900.0);
    crate::app::with_app_r(&fixture.app, |app| {
        app.doc
            .create_node(CanvasNodeType::Text, drop_world, app.now_ms())
    })
    .expect("the app is busy");
    let before = fixture.node_ids();
    fixture.drag_and_drop(drop_world);
    let after = fixture.node_ids();
    assert_eq!(after.len(), before.len() + 1);
    let created = after
        .iter()
        .find(|id| !before.contains(id))
        .expect("a new node");
    let node = fixture.node(created).expect("the created node");
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - drop_world.x).abs() < 2.0 && (centre.1 - drop_world.y).abs() < 2.0,
        "the card stays exactly at the release point ({centre:?})"
    );
    Ok(())
}

/// Releasing the drag back over the assets card it started from is a cancel:
/// no node, no leftover payload.
#[test]
fn assets_drag_release_back_on_the_card_creates_nothing() -> Result<(), Box<dyn std::error::Error>>
{
    use slint::Model;

    let fixture = assets_fixture()?;
    let before = fixture.node_ids();
    let rows_before = fixture.ui.get_nodes().row_count();
    fixture.drag_and_drop_back_on_the_card();
    assert_eq!(fixture.node_ids(), before, "no node is created");
    assert_eq!(fixture.ui.get_nodes().row_count(), rows_before);
    crate::app::with_app(&fixture.app, |app| {
        assert!(app.asset_drag.is_none());
        assert!(
            app.pending_media.is_empty(),
            "the prepared payload is discarded"
        );
    });
    assert_eq!(fixture.drag_node(), None, "the temporary node was removed");
    Ok(())
}

/// A release over an image stack keeps the existing drop rule: the dropped
/// image is appended to the stack and the transient card is consumed.
#[test]
fn assets_drag_drop_on_an_image_stack_appends_to_it() -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let drop_world = crate::coords::WorldPos::new(1700.0, 900.0);
    let stack = crate::app::with_app_r(&fixture.app, |app| {
        app.doc
            .create_node(CanvasNodeType::ImageStack, drop_world, app.now_ms())
    })
    .expect("the app is busy");
    let before = fixture.node_ids();
    fixture.drag_and_drop(drop_world);
    let after = fixture.node_ids();
    assert_eq!(
        after.len(),
        before.len(),
        "the dropped card is absorbed, not left on the canvas"
    );
    assert!(after.contains(&stack), "the stack survives");
    let images = fixture
        .node(&stack)
        .and_then(|node| node.metadata)
        .map(|meta| (meta.images.unwrap_or_default(), meta.count))
        .expect("the stack has metadata");
    assert_eq!(images.0.len(), 1, "the image is appended to the stack");
    assert_eq!(images.0[0].content, dropped_content(&fixture.bytes));
    assert_eq!(images.1, Some(1));
    Ok(())
}

/// End to end for the common raster formats: the tree lists every extension the
/// drag path decodes, and dragging each row creates an Image node whose data URL
/// carries the sniffed MIME type and bytes the media decoder can read.
#[test]
fn every_supported_raster_is_listed_and_drags_into_a_decodable_image(
) -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    crate::config_test_support::init_backend()?;
    let (_store_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let folder = tempfile::tempdir()?;

    let formats = [
        ("hero.png", image::ImageFormat::Png, "image/png"),
        ("photo.jpg", image::ImageFormat::Jpeg, "image/jpeg"),
        ("photo.jpeg", image::ImageFormat::Jpeg, "image/jpeg"),
        ("art.webp", image::ImageFormat::WebP, "image/webp"),
        ("anim.gif", image::ImageFormat::Gif, "image/gif"),
        ("scan.bmp", image::ImageFormat::Bmp, "image/bmp"),
        ("scan.tif", image::ImageFormat::Tiff, "image/tiff"),
        ("scan.tiff", image::ImageFormat::Tiff, "image/tiff"),
        ("icon.ico", image::ImageFormat::Ico, "image/x-icon"),
        ("paint.tga", image::ImageFormat::Tga, "image/x-tga"),
    ];
    for (name, format, _) in formats {
        let bytes = raster_fixture(format)?;
        std::fs::write(folder.path().join(name), bytes)?;
    }
    std::fs::write(folder.path().join("notes.txt"), b"not an asset")?;
    std::fs::write(folder.path().join("clip.avif"), b"not linked")?;

    // Every supported extension is listed by the tree filter; unsupported ones
    // (and a removed adjacent format) stay out, so they can never be dragged.
    let scan = crate::asset_folder::scan_folder(folder.path(), &std::collections::BTreeSet::new());
    let listed: Vec<String> = scan.rows.iter().map(|row| row.path.clone()).collect();
    for (name, _, _) in formats {
        assert!(
            listed.iter().any(|path| path == name),
            "{name} is missing from the tree: {listed:?}"
        );
    }
    assert!(!listed.iter().any(|path| path == "notes.txt"));
    assert!(!listed.iter().any(|path| path == "clip.avif"));

    let assets_id = crate::app::with_app_r(&app, |app| {
        app.doc = crate::doc::Doc::assemble(Vec::new(), Vec::new());
        let id = app.doc.create_node(
            CanvasNodeType::Assets,
            crate::coords::WorldPos::new(1500.0, 1200.0),
            app.now_ms(),
        );
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .and_then(|node| node.metadata.as_mut())
        {
            meta.asset_folder = Some(folder.path().to_string_lossy().to_string());
        }
        id
    })
    .ok_or("the app is busy")?;

    // The panel is closed so window and canvas coordinates coincide; the drop
    // point is empty canvas far from the assets card.
    ui.set_side_panel_open(false);
    for (name, _, mime) in formats {
        let before: Vec<String> = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().iter().map(|node| node.id.clone()).collect()
        })
        .unwrap_or_default();
        crate::app::with_app(&app, |app| {
            app.asset_drag_begin(&ui, &assets_id, name, 400.0, 400.0);
            assert!(app.asset_drag.is_some(), "{name} must arm the drag");
            app.asset_drag_file(&ui, &assets_id, name);
        });
        let after: Vec<String> = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().iter().map(|node| node.id.clone()).collect()
        })
        .unwrap_or_default();
        let created = after
            .iter()
            .find(|id| !before.contains(id))
            .unwrap_or_else(|| panic!("{name} must create one node"));
        let node = crate::app::with_app_r(&app, |app| app.doc.node(created).cloned())
            .flatten()
            .unwrap_or_else(|| panic!("{name}: the created node exists"));
        assert_eq!(node.node_type, CanvasNodeType::Image, "{name}");
        let meta = node
            .metadata
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: meta"));
        assert_eq!(meta.mime_type.as_deref(), Some(mime), "{name}: mime");
        let content = meta
            .content
            .as_deref()
            .unwrap_or_else(|| panic!("{name}: content"));
        assert!(
            content.starts_with(&format!("data:{mime};base64,")),
            "{name}: data URL carries its MIME type ({content:.40})"
        );
        let pixels = crate::media::decode_pixels(content)
            .unwrap_or_else(|| panic!("{name}: the stored bytes decode"));
        assert_eq!((pixels.width, pixels.height), (4, 3), "{name}");
        assert_eq!(
            meta.natural_width,
            Some(4.0),
            "{name}: decoded dimensions reach the node"
        );
    }
    Ok(())
}

/// An extension the tree does not list (and the drag path does not classify)
/// arms neither the ghost nor the drop; a direct release still creates nothing.
#[test]
fn an_unsupported_extension_is_not_draggable() -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    crate::config_test_support::init_backend()?;
    let (_store_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let folder = tempfile::tempdir()?;
    std::fs::write(folder.path().join("notes.txt"), b"not an asset")?;
    let assets_id = crate::app::with_app_r(&app, |app| {
        app.doc = crate::doc::Doc::assemble(Vec::new(), Vec::new());
        let id = app.doc.create_node(
            CanvasNodeType::Assets,
            crate::coords::WorldPos::new(1500.0, 1200.0),
            app.now_ms(),
        );
        if let Some(meta) = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .and_then(|node| node.metadata.as_mut())
        {
            meta.asset_folder = Some(folder.path().to_string_lossy().to_string());
        }
        id
    })
    .ok_or("the app is busy")?;
    ui.set_side_panel_open(false);
    let before = crate::app::with_app_r(&app, |app| app.doc.nodes().len()).unwrap_or(0);
    crate::app::with_app(&app, |app| {
        app.asset_drag_begin(&ui, &assets_id, "notes.txt", 400.0, 400.0);
        assert!(
            app.asset_drag.is_none(),
            "no temporary node for an unknown type"
        );
        app.asset_drag_file(&ui, &assets_id, "notes.txt");
    });
    let after = crate::app::with_app_r(&app, |app| app.doc.nodes().len()).unwrap_or(0);
    assert_eq!(after, before, "an unsupported file creates no node");
    Ok(())
}

// ---------------------------------------------------------------------------
// Canvas-level release: the row owns the drag origin, the canvas owns the drop
// ---------------------------------------------------------------------------

/// A pointer-up the canvas receives while a row drag is pending (the tree has
/// already handed the pointer back) creates exactly one image node at that
/// canvas point, from the dragged file, and clears the ghost state.
#[test]
fn canvas_release_with_a_pending_asset_drag_creates_the_image(
) -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let drop_world = crate::coords::WorldPos::new(1700.0, 900.0);
    let point = fixture.begin_drag_to(drop_world);
    assert!(fixture.drag_pending(), "the drag is in flight");
    let preview = fixture.drag_node().expect("the preview node exists");
    assert_eq!(preview.node_type, CanvasNodeType::Image);
    assert_eq!(
        (
            preview.position.x + preview.width / 2.0,
            preview.position.y + preview.height / 2.0
        ),
        (drop_world.x, drop_world.y),
        "the real node is the preview and sits at the pointer"
    );
    let created_id = preview.id.clone();

    fixture.canvas_up(point);

    let after = fixture.node_ids();
    assert!(
        after.contains(&created_id),
        "the release keeps the node it was dragging"
    );
    let node = fixture.node(&created_id).expect("the created node exists");
    assert_eq!(node.node_type, CanvasNodeType::Image);
    assert_eq!(
        node.metadata.as_ref().and_then(|meta| meta.content.clone()),
        Some(dropped_content(&fixture.bytes)),
        "the node carries the dragged file's content"
    );
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - drop_world.x).abs() < 0.5 && (centre.1 - drop_world.y).abs() < 0.5,
        "the card is centred on the canvas release point ({centre:?})"
    );
    assert!(!fixture.drag_pending(), "the drag is consumed");
    assert!(
        !fixture.drag_pending() && fixture.pending_payloads() == 0,
        "no leftover payload"
    );
    assert_eq!(
        fixture.ui.get_nodes().row_count(),
        after.len(),
        "the canvas model gained the new card"
    );
    Ok(())
}

/// A canvas release over an existing card still lands the image exactly under
/// the pointer: the preview was already placed there by the user.
#[test]
fn canvas_release_over_an_existing_node_keeps_the_pointer_position(
) -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let drop_world = crate::coords::WorldPos::new(1700.0, 900.0);
    crate::app::with_app_r(&fixture.app, |app| {
        app.doc
            .create_node(CanvasNodeType::Text, drop_world, app.now_ms())
    })
    .expect("the app is busy");
    let point = fixture.begin_drag_to(drop_world);
    let preview = fixture.drag_node().expect("the preview node");
    let created = preview.id.clone();

    fixture.canvas_up(point);

    let node = fixture.node(&created).expect("the released node");
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - drop_world.x).abs() < 2.0 && (centre.1 - drop_world.y).abs() < 2.0,
        "the card stays at the canvas release point ({centre:?})"
    );
    assert!(!fixture.drag_pending());
    Ok(())
}

/// Releasing over any assets card (here the source) is a cancel: no node, no
/// leftover payload.
#[test]
fn canvas_release_over_an_assets_card_creates_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = assets_fixture()?;
    let card = fixture.node(&fixture.assets_id).expect("the assets node");
    let centre = crate::coords::WorldPos::new(
        card.position.x + card.width / 2.0,
        card.position.y + card.height / 2.0,
    );
    let before = fixture.node_ids();
    let rows_before = fixture.ui.get_nodes().row_count();
    let point = fixture.begin_drag_to(centre);

    fixture.canvas_up(point);

    assert_eq!(fixture.node_ids(), before, "no node is created");
    assert_eq!(fixture.ui.get_nodes().row_count(), rows_before);
    assert!(!fixture.drag_pending(), "the drag is consumed");
    assert_eq!(fixture.pending_payloads(), 0, "the payload is discarded");
    assert_eq!(fixture.drag_node(), None, "the temporary node was removed");
    Ok(())
}

/// Without a pending drag the canvas release is an ordinary click: nothing is
/// created and the document is untouched.
#[test]
fn canvas_release_without_a_pending_drag_creates_nothing() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = assets_fixture()?;
    let before = fixture.node_ids();
    let point = crate::coords::ScreenPos::new(400.0, 300.0);

    fixture.canvas_up(point);

    assert_eq!(fixture.node_ids(), before, "no node is created");
    assert_eq!(fixture.pending_payloads(), 0);
    Ok(())
}

/// A cancelled row drag can stay pending when no release reaches the app; the
/// next canvas press clears it instead of dropping it on an unrelated click.
#[test]
fn a_new_canvas_press_clears_a_pending_asset_drag() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = assets_fixture()?;
    let before = fixture.node_ids();
    let point = fixture.begin_drag_to(crate::coords::WorldPos::new(1700.0, 900.0));

    fixture.canvas_pointer(crate::input::PointerKind::Down, point);

    assert_eq!(fixture.node_ids(), before, "the press creates nothing");
    assert!(!fixture.drag_pending(), "the stale drag is cleared");
    assert_eq!(fixture.drag_node(), None, "the temporary node was removed");
    assert_eq!(fixture.pending_payloads(), 0, "the payload is discarded");
    Ok(())
}

// ---------------------------------------------------------------------------
// Drop placement: one canvas point for the ghost and the created card
// ---------------------------------------------------------------------------

/// The centre of the one node the drop added.
fn created_centre(fixture: &AssetsFixture, before: &[String]) -> (f64, f64) {
    let after = fixture.node_ids();
    assert_eq!(after.len(), before.len() + 1, "the drop adds one node");
    let created = after
        .iter()
        .find(|id| !before.contains(id))
        .expect("a new node");
    let node = fixture.node(created).expect("the created node exists");
    (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    )
}

/// With the viewport panned and zoomed, both release paths (`drag-out` on the
/// row and the canvas `pointer-event` up) must map the canvas point through the
/// viewport: the new card is centred on the release point in world space.
#[test]
fn assets_release_lands_on_the_pointer_with_a_panned_zoomed_viewport(
) -> Result<(), Box<dyn std::error::Error>> {
    let viewport = crate::coords::Viewport {
        x: 260.0,
        y: -140.0,
        k: 1.75,
    };
    let point = crate::coords::WorldPos::new(900.0, 700.0);

    // The row still owns the pointer: `drag-out` resolves the last move.
    let row = assets_fixture()?;
    row.set_viewport(viewport);
    let before = row.node_ids();
    row.drag_and_drop(point);
    let (x, y) = created_centre(&row, &before);
    assert!(
        (x - point.x).abs() < 2.0 && (y - point.y).abs() < 2.0,
        "the row release lands at the pointer ({x}, {y} for {point:?})"
    );

    // The canvas owns the release: the pointer-up position is the drop.
    let canvas = assets_fixture()?;
    canvas.set_viewport(viewport);
    let release = canvas.begin_drag_to(point);
    let preview = canvas.drag_node().expect("the preview node");
    canvas.canvas_up(release);
    let node = canvas.node(&preview.id).expect("the released node");
    let (x, y) = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (x - point.x).abs() < 2.0 && (y - point.y).abs() < 2.0,
        "the canvas release lands at the pointer ({x}, {y} for {point:?})"
    );
    Ok(())
}

/// The preview node is painted inside the canvas layer, so its position must be
/// canvas-local: the row reports window coordinates (its `absolute-position`),
/// and the panel width must be removed exactly once — otherwise the card sits a
/// whole panel away from the cursor while the drop lands under it.
#[test]
fn assets_drag_node_stays_under_the_pointer_across_the_side_panel(
) -> Result<(), Box<dyn std::error::Error>> {
    fn centre(fixture: &AssetsFixture, id: &str) -> (f64, f64) {
        let node = fixture.node(id).expect("the preview node");
        (
            node.position.x + node.width / 2.0,
            node.position.y + node.height / 2.0,
        )
    }

    let fixture = assets_fixture()?;
    fixture.set_panel(true, 300.0);
    let preview = crate::app::with_app_r(&fixture.app, |app| {
        app.asset_drag_begin(
            &fixture.ui,
            &fixture.assets_id,
            &fixture.file_name,
            420.0,
            240.0,
        );
        app.asset_drag.as_ref().map(|drag| drag.node_id.clone())
    })
    .flatten()
    .expect("the drag created its preview");
    assert_eq!(
        centre(&fixture, &preview),
        (120.0, 240.0),
        "an open 300px panel is subtracted from the row's window x"
    );

    // A move is recorded, then applied once by the frame tick.
    crate::app::with_app(&fixture.app, |app| {
        app.asset_drag_moved(&fixture.ui, 820.0, 90.0);
    });
    assert_eq!(
        centre(&fixture, &preview),
        (120.0, 240.0),
        "no write before the tick"
    );
    crate::app::with_app(&fixture.app, |app| app.node_drag_tick(&fixture.ui));
    let (x, y) = centre(&fixture, &preview);
    assert!(
        (x - 520.0).abs() < 10.0 && (y - 90.0).abs() < 10.0,
        "the panel width is removed once while dragging ({x}, {y})"
    );

    // Release lands exactly on the last pointer sample.
    crate::app::with_app(&fixture.app, |app| {
        app.asset_drag_file(&fixture.ui, &fixture.assets_id, &fixture.file_name);
    });
    assert_eq!(centre(&fixture, &preview), (520.0, 90.0));

    // Closing the panel moves the canvas origin to the window edge.
    fixture.set_panel(false, 300.0);
    let reopened = crate::app::with_app_r(&fixture.app, |app| {
        app.asset_drag_begin(
            &fixture.ui,
            &fixture.assets_id,
            &fixture.file_name,
            520.0,
            90.0,
        );
        app.asset_drag.as_ref().map(|drag| drag.node_id.clone())
    })
    .flatten()
    .expect("the second drag created its preview");
    assert_eq!(
        centre(&fixture, &reopened),
        (520.0, 90.0),
        "the same window point maps to the same canvas point"
    );
    Ok(())
}

/// The same canvas release point must produce the same world point whether the
/// side panel is open or closed: the panel only shifts the window position the
/// row reports, never the drop.
#[test]
fn assets_release_lands_on_the_same_world_point_with_the_panel_open_or_closed(
) -> Result<(), Box<dyn std::error::Error>> {
    let point = crate::coords::WorldPos::new(820.0, 640.0);

    let open = assets_fixture()?;
    open.set_panel(true, 300.0);
    let before = open.node_ids();
    open.drag_and_drop(point);
    let (open_x, open_y) = created_centre(&open, &before);

    let closed = assets_fixture()?;
    closed.set_panel(false, 300.0);
    let before = closed.node_ids();
    closed.drag_and_drop(point);
    let (closed_x, closed_y) = created_centre(&closed, &before);

    for (x, y) in [(open_x, open_y), (closed_x, closed_y)] {
        assert!(
            (x - point.x).abs() < 2.0 && (y - point.y).abs() < 2.0,
            "the panel state does not move the world drop point ({x}, {y})"
        );
    }
    assert!(
        (open_x - closed_x).abs() < 0.5 && (open_y - closed_y).abs() < 0.5,
        "both panel states land on the same world point"
    );
    Ok(())
}

/// A drop over an occupied spot keeps the pointer position: the live preview is
/// the placement, so release never jumps the card away from the cursor.
#[test]
fn assets_drop_over_a_card_keeps_the_pointer_position() -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let point = crate::coords::WorldPos::new(900.0, 700.0);
    crate::app::with_app_r(&fixture.app, |app| {
        app.doc
            .create_node(CanvasNodeType::Text, point, app.now_ms())
    })
    .expect("the app is busy");
    let before = fixture.node_ids();

    fixture.drag_and_drop(point);

    let after = fixture.node_ids();
    assert_eq!(after.len(), before.len() + 1);
    let created = after
        .iter()
        .find(|id| !before.contains(id))
        .expect("a new node");
    let node = fixture.node(created).expect("the created node");
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - point.x).abs() < 2.0 && (centre.1 - point.y).abs() < 2.0,
        "the card stays at the pointer over an occupied spot ({centre:?})"
    );
    Ok(())
}

/// A release over a card arrives as that card's pointer-up (not the
/// background's). A pending row drag must resolve there too, at the card's
/// canvas point — otherwise the image silently never materialises.
#[test]
fn assets_release_over_a_card_resolves_through_the_card_pointer(
) -> Result<(), Box<dyn std::error::Error>> {
    use open_canvas_core::node::CanvasNodeType;

    let fixture = assets_fixture()?;
    let point = crate::coords::WorldPos::new(900.0, 700.0);
    let blocker_id = crate::app::with_app_r(&fixture.app, |app| {
        app.doc
            .create_node(CanvasNodeType::Text, point, app.now_ms())
    })
    .expect("the app is busy");
    let release = fixture.begin_drag_to(point);
    let preview = fixture.drag_node().expect("the preview node");

    let event = crate::input::PointerEvent {
        kind: crate::input::PointerKind::Up,
        pos: release,
        button: crate::input::Button::Left,
        shift: false,
        ctrl: false,
    };
    crate::app::with_app(&fixture.app, |app| {
        app.node_pointer(&fixture.ui, &blocker_id, &event);
    });

    let node = fixture
        .node(&preview.id)
        .expect("the release keeps the image");
    let centre = (
        node.position.x + node.width / 2.0,
        node.position.y + node.height / 2.0,
    );
    assert!(
        (centre.0 - point.x).abs() < 2.0 && (centre.1 - point.y).abs() < 2.0,
        "the card is landed at the card-release point ({centre:?})"
    );
    assert!(!fixture.drag_pending(), "the drag is consumed");
    Ok(())
}
