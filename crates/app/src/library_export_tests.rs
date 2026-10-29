//! Headless export/import tests driving the real flow: the export stages the
//! worker job and waits for the written archive, the import unpacks and
//! materialises exactly as `poll_archive` does. A project with a real bitmap
//! round-trips (bytes and references intact), a files-less project keeps today's
//! envelope bytes, referenced studio projects travel and remap, and the modal
//! renders through the testing backend (pixel assertions only, project rule: no
//! GUI, no screenshots).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use open_canvas_core::document::LibraryIndex;
use open_canvas_core::node::{
    CanvasAssetSource, CanvasAssetStudioProjects, CanvasNodeData, CanvasNodeMetadata,
    CanvasNodeType, Position,
};
use slint::ComponentHandle;

use super::*;
use crate::app::App;
use crate::test_support::{differing_pixels, init_backend};
use crate::AppWindow;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// 2x2 PNG with one red pixel, as the app's own media store would carry it.
fn red_png_data_url() -> Result<String, Box<dyn std::error::Error>> {
    let mut surface = open_canvas_ps::Surface::new(2, 2);
    surface.set(0, 0, [220, 20, 20, 255]);
    let png = open_canvas_ps::encode_png(&surface)?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

fn image_node(id: &str, content: &str) -> CanvasNodeData {
    CanvasNodeData {
        id: id.to_string(),
        node_type: CanvasNodeType::Image,
        title: "Image".to_string(),
        position: Position::new(0.0, 0.0),
        width: 240.0,
        height: 180.0,
        metadata: Some(CanvasNodeMetadata {
            content: Some(content.to_string()),
            ..Default::default()
        }),
    }
}

/// An assets node referencing studio projects (`assetStudioProjects`).
fn assets_node(id: &str, image_ids: &[&str]) -> CanvasNodeData {
    CanvasNodeData {
        id: id.to_string(),
        node_type: CanvasNodeType::Assets,
        title: "Assets".to_string(),
        position: Position::new(0.0, 0.0),
        width: 300.0,
        height: 220.0,
        metadata: Some(CanvasNodeMetadata {
            asset_source: Some(CanvasAssetSource::Studio),
            asset_studio_projects: Some(CanvasAssetStudioProjects {
                image: Some(image_ids.iter().map(|id| (*id).to_string()).collect()),
                pixel: None,
            }),
            ..Default::default()
        }),
    }
}

fn project_with(id: &str, title: &str, nodes: Vec<CanvasNodeData>) -> CanvasProject {
    let mut project = CanvasProject::new(id, title, "2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z");
    project.nodes = nodes;
    project
}

/// A store rooted at `dir` holding `project` plus its library row.
fn store_with_project(
    dir: &Path,
    project: &CanvasProject,
) -> Result<Store, Box<dyn std::error::Error>> {
    let store = Store::at(dir.to_path_buf());
    store.save_project(project)?;
    let mut index = LibraryIndex::new();
    index.upsert_project(project);
    store.save_index(&index)?;
    Ok(store)
}

/// An image-studio file carrying one base64 PNG bitmap.
fn image_file(
    id: &str,
    title: &str,
    pixel: [u8; 4],
) -> Result<crate::image::ImageFile, Box<dyn std::error::Error>> {
    let mut state = crate::image::ImageState::default();
    state.project.id = id.to_string();
    state.project.title = title.to_string();
    let mut surface = open_canvas_ps::Surface::new(2, 2);
    surface.set(0, 0, pixel);
    let png = open_canvas_ps::encode_png(&surface)?;
    let mut bitmaps = std::collections::BTreeMap::new();
    bitmaps.insert("image:layer-1".to_string(), BASE64.encode(png));
    Ok(crate::image::ImageFile {
        project: state.project,
        bitmaps,
        assets: crate::image::assets::ImageAssets::default(),
    })
}

/// Pops a finished worker result, waiting up to `timeout` for the thread.
fn wait_for_archive(timeout: Duration) -> Option<(ArchiveDone, ExportFlow)> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(done) = take_archive_done() {
            return Some(done);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn export_flow(path: &Path) -> ExportFlow {
    ExportFlow {
        import: false,
        project_id: "canvas-1".to_string(),
        path: path.display().to_string(),
        message: String::new(),
        error: false,
    }
}

/// Runs the real export path headlessly: stage the worker job, wait for it, and
/// return its summary line (or the failure message).
fn export_to(app: &App, project_id: &str, path: &Path) -> Result<String, String> {
    let job = app.export_job(project_id, path.to_path_buf())?;
    start_archive_job(job, export_flow(path))?;
    let Some((done, _flow)) = wait_for_archive(Duration::from_secs(10)) else {
        return Err("the archive worker did not finish".into());
    };
    match done {
        ArchiveDone::Exported { summary } => Ok(summary),
        ArchiveDone::Failed(message) => Err(message),
        other @ ArchiveDone::Imported { .. } => {
            Err(format!("expected a finished export, got {other:?}"))
        }
    }
}

/// Runs the real import path headlessly: unpack (the worker's half) and
/// materialise (the half `poll_archive` runs on the UI thread).
fn import_bytes(app: &mut App, bytes: &[u8]) -> Result<ImportSummary, String> {
    let (export, studio) = unpack_archive(bytes)?;
    app.apply_import(export, &studio)
}

#[test]
fn export_import_round_trip_carries_the_bitmap_across_stores() -> TestResult {
    let source_dir = tempfile::tempdir()?;
    let data_url = red_png_data_url()?;
    let project = project_with("canvas-1", "Canvas", vec![image_node("image-1", &data_url)]);
    let source_store = store_with_project(source_dir.path(), &project)?;
    let source_app = App::new(source_store);
    let out = source_dir.path().join("exports").join("canvas-1.zip");
    let summary = export_to(&source_app, "canvas-1", &out)?;
    assert!(summary.contains("Exported 1 image"), "{summary}");
    assert!(summary.contains("canvas-1.zip"), "{summary}");

    let bytes = std::fs::read(&out)?;
    assert!(bytes.starts_with(b"PK"), "the export is a ZIP archive");
    // The ZIP carries the decoded bytes, not the inline base64 text.
    let payload = data_url.split_once(',').map_or("", |(_, payload)| payload);
    let png = BASE64.decode(payload)?;
    assert!(payload.len() > 8 && !png.is_empty());
    assert!(
        bytes
            .windows(png.len())
            .any(|window| window == png.as_slice()),
        "the archive stores the raw bitmap bytes"
    );
    assert!(
        !bytes
            .windows(payload.len())
            .any(|window| window == payload.as_bytes()),
        "the inline base64 text is not stored"
    );

    // A second machine: a fresh data directory, the same archive.
    let target_dir = tempfile::tempdir()?;
    let mut target_app = App::new(Store::at(target_dir.path().to_path_buf()));
    let summary = import_bytes(&mut target_app, &bytes)?;
    assert_eq!(summary.projects, 1);
    assert_eq!(summary.studio, 0);
    assert_eq!(summary.warnings, Vec::<String>::new());

    let row = target_app
        .library
        .index
        .project("canvas-1")
        .ok_or("the imported project has a library row")?;
    assert_eq!(row.title, "Canvas");
    let loaded = target_app.store.load_project("canvas-1")?;
    let content = loaded.nodes[0]
        .metadata
        .as_ref()
        .and_then(|meta| meta.content.as_deref());
    assert_eq!(
        content,
        Some(data_url.as_str()),
        "the bitmap is inlined again"
    );
    let pixels = crate::media::decode_pixels(content.unwrap_or_default())
        .ok_or("the restored bitmap decodes")?;
    assert_eq!(pixels.pixel(0, 0), [220, 20, 20, 255]);
    Ok(())
}

#[test]
fn files_less_project_keeps_todays_envelope_and_document_bytes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let project = project_with("plain-1", "Plain", Vec::new());
    let store = store_with_project(dir.path(), &project)?;
    let app = App::new(store);
    let out = dir.path().join("plain-1.zip");
    let summary = export_to(&app, "plain-1", &out)?;
    assert!(summary.contains("Exported 0 images"), "{summary}");

    let bytes = std::fs::read(&out)?;
    let (export, blobs) = unpack_export(&bytes)?;
    assert_eq!(blobs, Vec::<Blob>::new(), "no media, no blobs");
    assert_eq!(export.app, EXPORT_APP);
    assert_eq!(export.version, EXPORT_VERSION);
    assert_eq!(export.projects.len(), 1);
    assert_eq!(
        export.projects[0].files,
        Vec::<CanvasExportAsset>::new(),
        "a files-less project keeps `files: []`"
    );
    // The project document inside the archive is byte-identical to the stored
    // one: the archive changed the container, not the v3 document format.
    let stored = app.store.load_project("plain-1")?;
    assert_eq!(
        serde_json::to_string(&export.projects[0].project)?,
        serde_json::to_string(&stored)?
    );
    Ok(())
}

#[test]
fn studio_reference_travels_with_the_archive() -> TestResult {
    let source_dir = tempfile::tempdir()?;
    let store = Store::at(source_dir.path().to_path_buf());
    store.save_image(&image_file("img-1", "Board", [10, 200, 30, 255])?)?;
    let project = project_with(
        "canvas-1",
        "Canvas",
        vec![assets_node("assets-1", &["img-1"])],
    );
    let store = store_with_project(source_dir.path(), &project)?;
    let app = App::new(store);
    let out = source_dir.path().join("canvas-1.zip");
    let summary = export_to(&app, "canvas-1", &out)?;
    assert!(summary.contains("1 studio project"), "{summary}");
    let bytes = std::fs::read(&out)?;

    let target_dir = tempfile::tempdir()?;
    let mut target_app = App::new(Store::at(target_dir.path().to_path_buf()));
    let summary = import_bytes(&mut target_app, &bytes)?;
    assert_eq!(summary.projects, 1);
    assert_eq!(summary.studio, 1);

    let row = target_app
        .library
        .index
        .project("img-1")
        .ok_or("the studio project has a library row")?;
    assert_eq!(row.kind, crate::image::PROJECT_KIND_IMAGE);
    assert_eq!(row.title, "Board");
    let file = target_app.store.load_image("img-1")?;
    let blob = file
        .bitmaps
        .get("image:layer-1")
        .ok_or("the studio bitmap is carried")?;
    let buffer = open_canvas_ps::decode_rgba(&BASE64.decode(blob)?)?;
    assert_eq!(
        buffer.get(0, 0),
        [10, 200, 30, 255],
        "the studio bitmap survives the archive"
    );
    // The reference is unchanged: the id was free on this machine.
    let canvas = target_app.store.load_project("canvas-1")?;
    let selection = canvas.nodes[0]
        .metadata
        .as_ref()
        .and_then(|meta| meta.asset_studio_projects.clone())
        .unwrap_or_default();
    assert_eq!(selection.image, Some(vec!["img-1".to_string()]));
    Ok(())
}

#[test]
fn colliding_studio_id_is_reminted_and_the_reference_remapped() -> TestResult {
    let source_dir = tempfile::tempdir()?;
    let store = Store::at(source_dir.path().to_path_buf());
    store.save_image(&image_file("img-1", "Incoming", [10, 200, 30, 255])?)?;
    let project = project_with(
        "canvas-1",
        "Canvas",
        vec![assets_node("assets-1", &["img-1"])],
    );
    let store = store_with_project(source_dir.path(), &project)?;
    let out = source_dir.path().join("canvas-1.zip");
    export_to(&App::new(store), "canvas-1", &out)?;
    let bytes = std::fs::read(&out)?;

    // The target machine already has an unrelated `img-1`.
    let target_dir = tempfile::tempdir()?;
    let target_store = Store::at(target_dir.path().to_path_buf());
    target_store.save_image(&image_file("img-1", "Existing", [1, 2, 3, 255])?)?;
    let existing = crate::image::ImageState::default();
    let mut index = LibraryIndex::new();
    index.upsert_entry(ProjectIndexEntry {
        id: "img-1".to_string(),
        title: "Existing".to_string(),
        created_at: existing.project.created_at.clone(),
        updated_at: existing.project.updated_at.clone(),
        group_id: None,
        kind: crate::image::PROJECT_KIND_IMAGE.to_string(),
    });
    target_store.save_index(&index)?;

    let mut target_app = App::new(target_store);
    let summary = import_bytes(&mut target_app, &bytes)?;
    assert_eq!(summary.studio, 1, "the incoming project is materialised");
    let rows: Vec<&str> = target_app
        .library
        .index
        .projects
        .iter()
        .filter(|row| crate::image::is_image_kind(&row.kind))
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(rows.len(), 2, "both image projects exist");
    assert!(rows.contains(&"img-1"), "the existing project is untouched");
    let new_id = rows
        .iter()
        .copied()
        .find(|id| *id != "img-1")
        .ok_or("the incoming project got a fresh id")?;
    assert!(new_id.starts_with("image-imported-"), "{new_id}");

    let existing_file = target_app.store.load_image("img-1")?;
    assert_eq!(existing_file.project.title, "Existing");
    let canvas = target_app.store.load_project("canvas-1")?;
    let selection = canvas.nodes[0]
        .metadata
        .as_ref()
        .and_then(|meta| meta.asset_studio_projects.clone())
        .unwrap_or_default();
    assert_eq!(
        selection.image,
        Some(vec![(*new_id).to_string()]),
        "the assets reference follows the re-minted id"
    );
    Ok(())
}

#[test]
fn missing_studio_reference_is_reported_not_fatal() -> TestResult {
    let dir = tempfile::tempdir()?;
    let project = project_with(
        "canvas-1",
        "Canvas",
        vec![assets_node("assets-1", &["img-gone"])],
    );
    let store = store_with_project(dir.path(), &project)?;
    let out = dir.path().join("canvas-1.zip");
    let summary = export_to(&App::new(store), "canvas-1", &out)?;
    assert!(
        summary.contains("skipped: studio image img-gone"),
        "{summary}"
    );
    // The archive is still valid: the reference stays a reference.
    let bytes = std::fs::read(&out)?;
    let (export, blobs) = unpack_export(&bytes)?;
    assert_eq!(blobs, Vec::<Blob>::new());
    assert_eq!(export.projects.len(), 1);
    Ok(())
}

#[test]
fn import_remints_a_taken_canvas_id() -> TestResult {
    let dir = tempfile::tempdir()?;
    let project = project_with("canvas-1", "Canvas", vec![image_node("image-1", "image:a")]);
    let store = store_with_project(dir.path(), &project)?;
    let out = dir.path().join("canvas-1.zip");
    export_to(&App::new(store.clone()), "canvas-1", &out)?;
    let bytes = std::fs::read(&out)?;

    let mut target_app = App::new(store);
    let summary = import_bytes(&mut target_app, &bytes)?;
    assert_eq!(summary.projects, 1);
    assert!(target_app.library.index.project("canvas-1").is_some());
    assert_eq!(target_app.library.index.projects.len(), 2);
    assert!(
        target_app
            .library
            .index
            .projects
            .iter()
            .any(|row| row.id.starts_with("project-imported-")),
        "the taken id is re-minted"
    );
    Ok(())
}

#[test]
fn worker_writes_the_archive_and_reports_back() -> TestResult {
    let dir = tempfile::tempdir()?;
    let project = project_with(
        "canvas-1",
        "Canvas",
        vec![image_node("image-1", &red_png_data_url()?)],
    );
    let store = store_with_project(dir.path(), &project)?;
    let app = App::new(store);
    let path = dir.path().join("exports").join("canvas-1.zip");
    let job = app.export_job("canvas-1", path.clone())?;
    let flow = export_flow(&path);
    start_archive_job(job, flow.clone())?;
    assert!(
        start_archive_job(app.export_job("canvas-1", path.clone())?, flow).is_err(),
        "a second job is refused while one runs"
    );

    let Some((done, reported)) = wait_for_archive(Duration::from_secs(10)) else {
        return Err("the archive worker did not finish".into());
    };
    assert_eq!(reported, export_flow(&path));
    match done {
        ArchiveDone::Exported { summary } => {
            assert!(summary.contains("Exported 1 image"), "{summary}");
        }
        other => return Err(format!("expected a finished export, got {other:?}").into()),
    }
    let bytes = std::fs::read(&path)?;
    assert!(bytes.starts_with(b"PK"));
    let (export, blobs) = unpack_export(&bytes)?;
    assert_eq!(export.projects.len(), 1);
    assert_eq!(blobs.len(), 1);
    Ok(())
}

#[test]
fn stack_entries_and_video_posters_travel_too() -> TestResult {
    let dir = tempfile::tempdir()?;
    let entry_a = red_png_data_url()?;
    let entry_b = {
        let mut surface = open_canvas_ps::Surface::new(2, 2);
        surface.set(0, 0, [10, 200, 30, 255]);
        format!(
            "data:image/png;base64,{}",
            BASE64.encode(open_canvas_ps::encode_png(&surface)?)
        )
    };
    let mut stack = image_node("stack-1", "");
    stack.node_type = CanvasNodeType::ImageStack;
    let stack_meta = stack.metadata.as_mut().ok_or("stack metadata")?;
    stack_meta.content = None;
    stack_meta.images = Some(vec![
        open_canvas_core::node::CanvasNodeImage {
            id: "a".to_string(),
            content: entry_a.clone(),
            ..Default::default()
        },
        open_canvas_core::node::CanvasNodeImage {
            id: "b".to_string(),
            content: entry_b.clone(),
            ..Default::default()
        },
    ]);
    let mut video = image_node("video-1", "https://example.test/clip.mp4");
    video.node_type = CanvasNodeType::Video;
    video.metadata.as_mut().ok_or("video metadata")?.thumbnail = Some(entry_a.clone());

    let project = project_with("canvas-1", "Canvas", vec![stack, video]);
    let store = store_with_project(dir.path(), &project)?;
    let out = dir.path().join("canvas-1.zip");
    let summary = export_to(&App::new(store), "canvas-1", &out)?;
    // The poster repeats the stack's first payload, and identical payloads
    // share one archive member: two distinct bitmaps, not three.
    assert!(summary.contains("Exported 2 images"), "{summary}");
    let bytes = std::fs::read(&out)?;

    let target_dir = tempfile::tempdir()?;
    let mut target_app = App::new(Store::at(target_dir.path().to_path_buf()));
    let summary = import_bytes(&mut target_app, &bytes)?;
    assert_eq!(summary.projects, 1);
    let loaded = target_app.store.load_project("canvas-1")?;
    let stack = loaded
        .nodes
        .iter()
        .find(|node| node.id == "stack-1")
        .ok_or("the stack node is imported")?;
    let images = stack
        .metadata
        .as_ref()
        .and_then(|meta| meta.images.clone())
        .unwrap_or_default();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].content, entry_a);
    assert_eq!(images[1].content, entry_b);
    let video = loaded
        .nodes
        .iter()
        .find(|node| node.id == "video-1")
        .ok_or("the video node is imported")?;
    assert_eq!(
        video
            .metadata
            .as_ref()
            .and_then(|meta| meta.thumbnail.as_deref()),
        Some(entry_a.as_str()),
        "the video poster is inlined again"
    );
    Ok(())
}

#[test]
fn import_modal_paints_over_the_shell_and_its_error_line() -> TestResult {
    init_backend()?;
    let dir = tempfile::tempdir()?;
    let project = project_with("canvas-1", "Canvas", Vec::new());
    let store = store_with_project(dir.path(), &project)?;
    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App::new(store)));
    crate::events::wire(&ui, &app);
    crate::app::with_app(&app, |app| app.sync_all(&ui));
    ui.set_view("editor".into());
    ui.show()?;
    ui.window().request_redraw();
    let baseline = ui.window().take_snapshot()?;

    crate::app::with_app(&app, |app| app.open_import_modal(&ui));
    ui.window().request_redraw();
    let open = ui.window().take_snapshot()?;
    let painted = differing_pixels(&baseline, &open).map_err(ToString::to_string)?;
    assert!(
        painted > 20_000,
        "the import modal paints its scrim and panel ({painted})"
    );

    crate::app::with_app(&app, |app| {
        app.set_export_message(&ui, "Import failed: fixture".to_string(), true);
    });
    ui.window().request_redraw();
    let failed = ui.window().take_snapshot()?;
    let message = differing_pixels(&open, &failed).map_err(ToString::to_string)?;
    assert!(message > 100, "the error line paints ({message})");
    Ok(())
}
