//! `Store` unit tests: JSON round-trips through the real filesystem (temp dirs),
//! id/path safety and error surfacing. Every fallible setup step propagates via
//! `?` — no `unwrap`/`expect` in the app crate.

use std::error::Error;

use open_canvas_core::document::{
    CanvasExportFile, CanvasGroup, CanvasProjectExportItem, DocumentError, EXPORT_APP,
    EXPORT_VERSION,
};
use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeMetadata, CanvasNodeType, Position,
};

use super::*;

fn temp_store() -> Result<(tempfile::TempDir, Store), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let store = Store::at(dir.path().to_path_buf());
    Ok((dir, store))
}

fn sample_project(id: &str, title: &str) -> CanvasProject {
    let mut project = CanvasProject::new(id, title, "2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z");
    project.nodes.push(CanvasNodeData {
        id: "text-1".into(),
        node_type: CanvasNodeType::Text,
        title: "Text".into(),
        position: Position::new(64.0, 96.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(CanvasNodeMetadata {
            content: Some("hello 世界".into()),
            font_size: Some(16.0),
            ..Default::default()
        }),
    });
    project.connections.push(CanvasConnection {
        id: "conn-1".into(),
        from_node_id: "text-1".into(),
        to_node_id: "text-1".into(),
        relation: Some("reference".into()),
        kind: None,
        reason: None,
        changes: None,
        agent_run_id: None,
    });
    project.viewport = open_canvas_core::document::ViewportTransform {
        x: -12.5,
        y: 40.0,
        k: 1.25,
    };
    project.group_id = Some("group-1".into());
    project
}

#[test]
fn index_round_trips_through_disk() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let mut index = LibraryIndex::new();
    index.groups.push(CanvasGroup {
        id: "group-1".into(),
        name: "Product".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    });
    index.upsert_project(&sample_project("project-1", "Canvas 1"));
    store.save_index(&index)?;

    let loaded = store.load_index()?;
    assert_eq!(loaded, index);
    Ok(())
}

#[test]
fn project_round_trips_through_disk_losslessly() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let project = sample_project("project-1", "My Canvas");
    store.save_project(&project)?;

    let loaded = store.load_project("project-1")?;
    assert_eq!(loaded, project);
    Ok(())
}

#[test]
fn missing_index_is_empty_and_missing_project_is_an_error() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let index = store.load_index()?;
    assert_eq!(index, LibraryIndex::new());
    assert_eq!(index.projects.len(), 0);

    // A row whose file vanished still fails explicitly instead of loading junk.
    assert!(matches!(
        store.load_project("ghost"),
        Err(StoreError::Io { .. })
    ));
    Ok(())
}

#[test]
fn export_round_trips_and_rejects_foreign_envelopes() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let export = CanvasExportFile {
        app: EXPORT_APP.to_string(),
        version: EXPORT_VERSION,
        exported_at: "2026-01-02T00:00:00Z".into(),
        projects: vec![CanvasProjectExportItem {
            project: sample_project("project-1", "My Canvas"),
            group_name: Some("Product".into()),
            files: Vec::new(),
        }],
    };
    let path = store.export_path_for("My Canvas");
    write_export(&path, &export)?;
    assert_eq!(read_export(&path)?, export);

    let foreign = path.with_file_name("foreign.json");
    std::fs::write(
        &foreign,
        r#"{ "app": "other", "version": 3, "exportedAt": "x", "projects": [] }"#,
    )?;
    assert!(matches!(
        read_export(&foreign),
        Err(StoreError::Document(DocumentError::InvalidApp { .. }))
    ));

    let old = path.with_file_name("old.json");
    std::fs::write(
        &old,
        r#"{ "app": "open-canvas", "version": 2, "exportedAt": "x", "projects": [] }"#,
    )?;
    assert!(matches!(
        read_export(&old),
        Err(StoreError::Document(DocumentError::InvalidVersion { .. }))
    ));
    Ok(())
}

#[test]
fn unsafe_project_ids_never_become_paths() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    assert!(matches!(
        store.project_path("../escape"),
        Err(StoreError::InvalidId(_))
    ));
    assert!(matches!(
        store.project_path("nested/id"),
        Err(StoreError::InvalidId(_))
    ));
    assert!(matches!(
        store.project_path(""),
        Err(StoreError::InvalidId(_))
    ));
    assert!(store.project_path("project-1234-5").is_ok());
    assert!(store.project_path("webNanoid9_x-1").is_ok());
    Ok(())
}

#[test]
fn delete_is_idempotent_and_export_stem_is_sanitized() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let project = sample_project("project-1", "My Canvas");
    store.save_project(&project)?;
    store.delete_project("project-1")?;
    store.delete_project("project-1")?;
    assert!(store.load_project("project-1").is_err());

    assert_eq!(safe_stem("A cozy cabin / dusk"), "A-cozy-cabin-dusk");
    assert_eq!(safe_stem("???"), "canvas");
    assert_eq!(display_time("2026-10-16T12:34:56Z"), "10-16 12:34");
    assert_eq!(display_time("nonsense"), "nonsense");
    Ok(())
}

#[test]
fn image_project_round_trips_through_disk_with_its_bitmaps() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let mut project = crate::image::ImageState::default().project;
    project.id = "image-1".into();
    project.title = "Image 1".into();
    project.width = 16.0;
    project.height = 16.0;
    project.board_ratio = "1:1".into();
    project.group_id = "group-1".into();
    let mut layer = open_canvas_ps::base_layer(
        "l1".into(),
        "Paint",
        open_canvas_ps::CanvasPsLayerKind::Pixel,
        open_canvas_ps::Bounds::new(0.0, 0.0, 16.0, 16.0),
    );
    layer.storage_key = Some("image:l1".into());
    project.board_layers = vec![layer];
    let mut bitmaps = std::collections::HashMap::new();
    let mut surface = open_canvas_ps::Surface::new(16, 16);
    surface.set(3, 4, [237, 28, 36, 255]);
    bitmaps.insert("image:l1".to_string(), surface);

    let file = crate::image::ImageFile::from_state(
        &project,
        &bitmaps,
        &crate::image::assets::ImageAssets::default(),
    )?;
    store.save_image(&file)?;
    assert!(store.image_path("image-1").is_ok());

    let (loaded, loaded_bitmaps, _assets) = store.load_image("image-1")?.into_state()?;
    assert_eq!(loaded, project);
    assert_eq!(
        loaded_bitmaps
            .get("image:l1")
            .map(|surface| surface.get(3, 4)),
        Some([237, 28, 36, 255])
    );

    store.delete_image("image-1")?;
    store.delete_image("image-1")?;
    assert!(store.load_image("image-1").is_err());
    assert!(matches!(
        store.image_path("../escape"),
        Err(StoreError::InvalidId(_))
    ));
    assert!(matches!(store.delete_by_kind("image-1", "image"), Ok(())));
    Ok(())
}

#[test]
fn ui_state_round_trips_and_clamps_through_disk() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    // A missing file yields the web defaults.
    let defaults = store.load_ui_state()?;
    assert_eq!(defaults.side_panel_width, SIDE_PANEL_DEFAULT_WIDTH);
    assert!(defaults.side_panel_open);

    let state = UiState {
        side_panel_width: 317.0,
        side_panel_open: false,
        ..UiState::default()
    };
    store.save_ui_state(&state)?;
    let loaded = store.load_ui_state()?;
    assert_eq!(loaded, state, "width + open flag survive the round trip");
    assert!(store.ui_state_path().ends_with("ui.json"));

    // Out-of-range persisted values clamp on load.
    std::fs::write(
        store.ui_state_path(),
        r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":9999,"sidePanelOpen":true}"#,
    )?;
    assert_eq!(
        store.load_ui_state()?.side_panel_width,
        SIDE_PANEL_MAX_WIDTH
    );
    std::fs::write(
        store.ui_state_path(),
        r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":10,"sidePanelOpen":true}"#,
    )?;
    assert_eq!(
        store.load_ui_state()?.side_panel_width,
        SIDE_PANEL_MIN_WIDTH
    );

    // Foreign or malformed files fall back to defaults instead of failing.
    std::fs::write(store.ui_state_path(), "{not json")?;
    assert_eq!(store.load_ui_state()?, UiState::default());
    std::fs::write(
        store.ui_state_path(),
        r#"{"app":"other-app","version":1,"sidePanelWidth":300,"sidePanelOpen":false}"#,
    )?;
    assert_eq!(store.load_ui_state()?, UiState::default());
    Ok(())
}

#[test]
fn legacy_window_fields_are_ignored_on_load() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    // Builds before the opaque window persisted tintOpacity/blur. The keys are
    // still accepted (serde ignores unknown fields) and never read: loading
    // succeeds and every real preference survives.
    std::fs::write(
        store.ui_state_path(),
        r#"{"app":"open-canvas-desktop-ui","version":1,"sidePanelWidth":300.0,"sidePanelOpen":false,"theme":"dark","locale":"en-US","tintOpacity":37.0,"blur":64.0}"#,
    )?;
    let loaded = store.load_ui_state()?;
    assert_eq!(loaded.side_panel_width, 300.0);
    assert!(!loaded.side_panel_open);
    assert_eq!(loaded.theme, THEME_DARK);
    Ok(())
}

#[test]
fn side_panel_width_clamp_handles_non_finite_values() {
    assert_eq!(clamp_side_panel_width(150.0), SIDE_PANEL_MIN_WIDTH);
    assert_eq!(clamp_side_panel_width(700.0), SIDE_PANEL_MAX_WIDTH);
    assert_eq!(clamp_side_panel_width(300.0), 300.0);
    assert_eq!(clamp_side_panel_width(f64::NAN), SIDE_PANEL_DEFAULT_WIDTH);
    assert_eq!(
        clamp_side_panel_width(f64::INFINITY),
        SIDE_PANEL_DEFAULT_WIDTH
    );
}

#[test]
fn cost_records_round_trip_and_a_missing_file_is_empty() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    let empty = store.load_costs()?;
    assert_eq!(empty, crate::cost::CostLog::default());
    assert!(store.costs_path().ends_with("costs.json"));

    let mut log = crate::cost::CostLog::default();
    log.record_estimate(
        "node-1",
        "gpt-image-1",
        open_canvas_ai::generation_cost::GenerationCostUnit::Image,
        2.0,
        42,
    );
    store.save_costs(&log)?;
    let loaded = store.load_costs()?;
    assert_eq!(loaded, log);

    // A malformed sidecar surfaces as a typed config error, never a panic.
    std::fs::write(store.costs_path(), "{not json")?;
    assert!(store.load_costs().is_err());
    Ok(())
}

#[test]
fn models_dir_sits_under_the_data_root() -> Result<(), Box<dyn Error>> {
    let (_dir, store) = temp_store()?;
    assert_eq!(store.models_dir(), store.root().join("models"));
    Ok(())
}
