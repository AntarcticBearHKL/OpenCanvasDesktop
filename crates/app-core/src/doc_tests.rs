use super::*;
use crate::gesture::{Corner, ResizeGesture, MIN_NODE_HEIGHT, MIN_NODE_WIDTH};
use open_canvas_core::node::{CanvasNodeType, ConnectionHandleType};

fn drag_doc() -> Doc {
    Doc::demo()
}

#[test]
fn zoom_anchor_keeps_cursor_world_point_fixed() {
    let mut doc = drag_doc();
    let cursor = ScreenPos::new(300.0, 200.0);
    let before = doc.viewport().screen_to_world(cursor);
    doc.zoom(cursor, 50.0);
    assert!((doc.viewport().screen_to_world(cursor).x - before.x).abs() < 1e-9);
}

#[test]
fn marquee_selects_intersecting_nodes_and_skips_locked() {
    let mut doc = drag_doc();
    // Rect (0,0)-(600,500) intersects Prompt, Image, Text and Video Prompt.
    doc.begin_marquee(WorldPos::new(0.0, 0.0), false);
    doc.marquee_to(WorldPos::new(600.0, 500.0));
    assert!(doc.selection().contains("prompt"));
    assert!(doc.selection().contains("image"));
    assert!(doc.selection().contains("text"));

    doc.end_marquee();
    doc.clear_selection();
    if let Some(node) = doc.nodes.iter_mut().find(|n| n.id == "prompt") {
        if let Some(meta) = node.metadata.as_mut() {
            meta.locked = Some(true);
        }
    }
    doc.begin_marquee(WorldPos::new(0.0, 0.0), false);
    doc.marquee_to(WorldPos::new(600.0, 500.0));
    assert!(!doc.selection().contains("prompt"));
    assert!(doc.selection().contains("image"));
}

#[test]
fn node_drag_snaps_to_a_neighbour_edge_and_reports_the_guide() {
    let mut doc = drag_doc();
    // Prompt left edge 64; drag Image (left edge 492) toward 64 within the 6px threshold.
    doc.begin_node_drag("image", ScreenPos::new(0.0, 0.0));
    assert!(doc.node_drag_to(ScreenPos::new(64.0 - 492.0 + 4.0, 3.0)));
    let image_x = doc
        .nodes()
        .iter()
        .find(|n| n.id == "image")
        .map(|n| n.position.x);
    assert_eq!(image_x, Some(64.0));
    assert_ne!(doc.snap_guides().x.len(), 0);
    assert!(doc.end_node_drag());
    assert_eq!(doc.snap_guides().x.len(), 0);
}

#[test]
fn locked_nodes_neither_drag_nor_delete() {
    let mut doc = drag_doc();
    if let Some(meta) = doc
        .nodes
        .iter_mut()
        .find(|n| n.id == "prompt")
        .and_then(|n| n.metadata.as_mut())
    {
        meta.locked = Some(true);
    }
    assert!(!doc.begin_node_drag("prompt", ScreenPos::new(0.0, 0.0)));
    doc.selection.insert("prompt".into());
    assert!(!doc.delete_selection(0));
    assert!(doc.node("prompt").is_some());
}

#[test]
fn resize_honours_min_size_and_ratio_lock() {
    let gesture = ResizeGesture {
        node_id: "image".into(),
        corner: Corner::BottomRight,
        start: ScreenPos::new(0.0, 0.0),
        x: 100.0,
        y: 100.0,
        width: 340.0,
        height: 240.0,
        keep_ratio: true,
        ratio: 340.0 / 240.0,
    };
    let (x, y, width, height) = gesture.bounds_at(ScreenPos::new(-1000.0, -1000.0), 1.0);
    assert_eq!((x, y), (100.0, 100.0));
    assert!(width >= MIN_NODE_WIDTH && height >= MIN_NODE_HEIGHT);
    assert!((width / height - 340.0 / 240.0).abs() < 1e-9);

    let free = ResizeGesture {
        keep_ratio: false,
        ..gesture.clone()
    };
    let (_, _, width, height) = free.bounds_at(ScreenPos::new(-1000.0, -1000.0), 1.0);
    assert_eq!((width, height), (MIN_NODE_WIDTH, MIN_NODE_HEIGHT));
}

#[test]
fn image_resize_ignores_the_legacy_free_resize_flag() {
    let mut doc = drag_doc();
    if let Some(meta) = doc
        .nodes
        .iter_mut()
        .find(|n| n.id == "image")
        .and_then(|n| n.metadata.as_mut())
    {
        meta.free_resize = Some(true);
    }
    let start = ScreenPos::new(0.0, 0.0);
    assert!(doc.begin_resize("image", Corner::BottomRight, start));
    assert!(doc.resize_to(ScreenPos::new(120.0, 10.0)));
    let node = doc.node("image").expect("image node");
    let ratio = node.width / node.height;
    assert!(
        (ratio - 340.0 / 240.0).abs() < 1e-9,
        "the card follows the bitmap ratio: {node:?}"
    );
    assert!(doc.end_resize());
}

#[test]
fn connect_drag_rejects_image_targets_and_dedups_legal_wires() {
    let mut doc = drag_doc();
    let before = doc.connections().len();
    doc.begin_connect("text", ConnectionHandleType::Source);
    // Dropping inside the image node is illegal: nothing may target an Image.
    doc.connect_to(WorldPos::new(600.0, 150.0));
    assert_eq!(doc.connect_target(), None);
    assert!(!doc.end_connect(10));
    // Dropping on the video-prompt node normalizes to text -> video-prompt.
    doc.begin_connect("text", ConnectionHandleType::Source);
    doc.connect_to(WorldPos::new(200.0, 500.0));
    assert_eq!(doc.connect_target(), Some("video-prompt"));
    assert!(doc.end_connect(20));
    assert_eq!(doc.connections().len(), before + 1);
    // The same drag again is deduplicated.
    doc.begin_connect("text", ConnectionHandleType::Source);
    doc.connect_to(WorldPos::new(200.0, 500.0));
    assert!(!doc.end_connect(30));
    assert_eq!(doc.connections().len(), before + 1);
}

#[test]
fn undo_restores_the_pre_change_snapshot() {
    let mut doc = drag_doc();
    let before = doc.nodes().len();
    doc.create_node(CanvasNodeType::Text, WorldPos::new(900.0, 200.0), 0);
    assert_eq!(doc.nodes().len(), before + 1);
    // The commit lands once the 180ms debounce window elapses.
    assert!(doc.tick_history(open_canvas_core::history::HISTORY_DEBOUNCE_MS));
    assert!(doc.undo());
    assert_eq!(doc.nodes().len(), before);
    assert!(doc.redo());
    assert_eq!(doc.nodes().len(), before + 1);
}

#[test]
fn document_and_project_convert_losslessly_through_the_save_shape() {
    use crate::doc_project::EditorMeta;
    use open_canvas_core::document::CanvasProject;

    let project = CanvasProject {
        id: "project-1".into(),
        title: "My Canvas".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-02T00:00:00Z".into(),
        group_id: Some("group-1".into()),
        nodes: Doc::demo().nodes().to_vec(),
        connections: Doc::demo().connections().to_vec(),
        viewport: open_canvas_core::document::ViewportTransform {
            x: -120.0,
            y: 64.0,
            k: 1.75,
        },
        scope: None,
    };
    let doc = Doc::from_project(&project);
    assert_eq!(doc.viewport().x, -120.0);
    assert_eq!(doc.viewport().k, 1.75);
    assert_eq!(doc.nodes(), project.nodes.as_slice());
    assert_eq!(doc.connections(), project.connections.as_slice());

    let meta = EditorMeta {
        id: project.id.clone(),
        title: project.title.clone(),
        created_at: project.created_at.clone(),
        updated_at: project.updated_at.clone(),
        group_id: project.group_id.clone(),
    };
    assert_eq!(doc.to_project(&meta), project);
}

#[test]
fn reset_viewport_centers_the_origin_at_full_scale() {
    let mut doc = drag_doc();
    doc.zoom(ScreenPos::new(10.0, 10.0), 600.0);
    doc.reset_viewport((1040.0, 744.0));
    assert_eq!(
        doc.viewport(),
        crate::coords::Viewport {
            x: 520.0,
            y: 372.0,
            k: 1.0,
        }
    );
}

#[test]
fn fit_to_content_frames_the_visible_bounds() {
    let mut doc = drag_doc();
    doc.fit_to_content((1040.0, 744.0));
    // Demo bounds: x 64..1260, y 48..1016 (width 1196, height 968).
    let expected = (1040.0 / (1196.0 + 2.0 * crate::nav::FIT_PADDING))
        .min(744.0 / (968.0 + 2.0 * crate::nav::FIT_PADDING));
    assert!((doc.viewport().k - expected).abs() < 1e-9);
    let center = doc.viewport().screen_to_world(ScreenPos::new(520.0, 372.0));
    assert!((center.x - 662.0).abs() < 1e-6);
    assert!((center.y - 532.0).abs() < 1e-6);

    for node in doc.nodes_mut() {
        if let Some(meta) = node.metadata.as_mut() {
            meta.hidden = Some(true);
        }
    }
    doc.fit_to_content((1040.0, 744.0));
    assert_eq!(
        doc.viewport().x,
        520.0,
        "an empty visible set resets the view"
    );
    assert_eq!(doc.viewport().k, 1.0);
}

#[test]
fn selecting_nodes_or_marqueeing_clears_a_wire_selection() {
    let mut doc = drag_doc();
    doc.select_connection(Some("conn-prompt-text"));
    doc.select_node("prompt", false);
    assert_eq!(doc.selected_connection(), None);

    doc.select_connection(Some("conn-prompt-text"));
    doc.begin_marquee(WorldPos::new(0.0, 0.0), false);
    assert_eq!(doc.selected_connection(), None);
}

#[test]
fn deleting_nodes_clears_a_wire_selection_whose_wire_is_gone() {
    let mut doc = drag_doc();
    doc.select_all();
    doc.select_connection(Some("conn-prompt-text"));
    assert!(doc.delete_selection(0));
    assert_eq!(
        doc.selected_connection(),
        None,
        "the wire selection dies with the wire's endpoint nodes"
    );
}

#[test]
fn node_drag_follows_the_pointer_without_grid_quantization() {
    let mut doc = drag_doc();
    doc.nodes.retain(|node| node.id == "image");
    doc.connections.clear();
    let before = doc.node("image").map(|node| node.position).expect("image");
    doc.begin_node_drag("image", ScreenPos::new(0.0, 0.0));
    assert!(doc.node_drag_to(ScreenPos::new(5.0, 7.0)));
    let after = doc.node("image").map(|node| node.position).expect("image");
    assert!((after.x - before.x - 5.0).abs() < 1e-9);
    assert!((after.y - before.y - 7.0).abs() < 1e-9);
    assert!(doc.snap_guides().x.is_empty());
    assert!(doc.snap_guides().y.is_empty());
}
