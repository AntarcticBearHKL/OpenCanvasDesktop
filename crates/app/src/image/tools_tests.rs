//! Headless tests of the canvas media pipeline: image node content, the image
//! quick tools on synthetic buffers (pixel assertions) and the stack gallery
//! actions.

use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType};

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A solid `width x height` red surface.
fn red_solid(width: usize, height: usize) -> Surface {
    let mut surface = Surface::new(width, height);
    surface.fill([220, 20, 20, 255]);
    surface
}

/// A `width x height` surface whose left `dominant` columns are red and the
/// rest blue (an asymmetric fixture so the palette ranking is deterministic).
fn dominant_surface(width: usize, height: usize, dominant: usize) -> Surface {
    let mut surface = Surface::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = if x < dominant {
                [220, 20, 20, 255]
            } else {
                [20, 20, 220, 255]
            };
            surface.set(crate::vm::u2i(x), crate::vm::u2i(y), pixel);
        }
    }
    surface
}

fn temp_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let store = crate::store::Store::at(dir.path().to_path_buf());
    let mut app = App::new(store);
    app.doc = crate::doc::Doc::empty();
    Ok((app, dir))
}

/// A `width x height` surface whose left half is red and right half blue.
fn split_surface(width: usize, height: usize) -> Surface {
    let mut surface = Surface::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = if x < width / 2 {
                [220, 20, 20, 255]
            } else {
                [20, 20, 220, 255]
            };
            surface.set(crate::vm::u2i(x), crate::vm::u2i(y), pixel);
        }
    }
    surface
}

fn image_node(app: &mut App, surface: &Surface) -> Option<String> {
    let now = app.now_ms();
    app.insert_image_node("Fixture", surface, WorldPos::new(0.0, 0.0), now)
}

// ---- Image nodes ------------------------------------------------------------

#[test]
fn an_image_node_from_a_surface_always_carries_its_bitmap() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(4, 2)).ok_or("fixture node")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    assert_eq!(node.node_type, CanvasNodeType::Image);
    let meta = node.metadata.as_ref().ok_or("metadata")?;
    assert_eq!(meta.status, Some(CanvasNodeStatus::Success));
    assert_eq!(meta.mime_type.as_deref(), Some("image/png"));
    let content = meta.content.clone().ok_or("content")?;
    assert!(content.starts_with("data:image/png;base64,"));
    let images = meta.images.clone().ok_or("images")?;
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].content, content);
    assert_eq!(
        meta.primary_image_id.as_deref(),
        Some(images[0].id.as_str())
    );
    assert!(images[0].bytes > 0, "the encoded size is recorded");
    // The card is centered on the insertion point and fitted to the natural size.
    assert_eq!((node.width, node.height), (4.0, 2.0));
    assert_eq!((node.position.x, node.position.y), (-2.0, -1.0));
    let decoded = node_surface(&node).ok_or("decoded surface")?;
    assert_eq!(decoded.get(0, 0), [220, 20, 20, 255]);
    assert_eq!(decoded.get(3, 1), [20, 20, 220, 255]);
    Ok(())
}

// ---- Crop -------------------------------------------------------------------

#[test]
fn crop_pixels_floor_the_origin_and_ceil_the_size() {
    // 100x50 image, rect x=0.125 -> 12.5 -> 12; width 0.25 -> 25 -> ceil 25.
    let rect = NormRect {
        x: 0.125,
        y: 0.2,
        width: 0.25,
        height: 0.5,
    };
    let pixels = crop_rect_pixels(rect, 100, 50);
    assert_eq!((pixels.x, pixels.y), (12, 10));
    assert_eq!((pixels.width, pixels.height), (25, 25));
    // A degenerate rect stays inside the source and keeps at least one pixel.
    let tiny = crop_rect_pixels(
        NormRect {
            x: 1.5,
            y: 1.5,
            width: 0.0,
            height: 0.0,
        },
        10,
        10,
    );
    assert_eq!((tiny.x, tiny.y), (9, 9));
    assert_eq!((tiny.width, tiny.height), (1, 1));
}

#[test]
fn cropping_a_synthetic_buffer_keeps_the_expected_half() {
    let source = split_surface(8, 4);
    let left = crop_rect_pixels(
        NormRect {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        },
        8,
        4,
    );
    let cropped = crop_surface_clamped(&source, left);
    assert_eq!((cropped.width(), cropped.height()), (4, 4));
    assert_eq!(cropped.get(0, 0), [220, 20, 20, 255]);
    assert_eq!(
        cropped.get(3, 3),
        [220, 20, 20, 255],
        "only the red half survives"
    );

    let right = crop_rect_pixels(
        NormRect {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        },
        8,
        4,
    );
    let cropped = crop_surface_clamped(&source, right);
    assert_eq!(cropped.get(0, 0), [20, 20, 220, 255]);
}

#[test]
fn crop_ratio_helpers_follow_the_dialog_math() {
    assert_eq!(crop_ratio("free", (100, 50), None), None);
    assert_eq!(crop_ratio("original", (100, 50), None), Some(2.0));
    assert_eq!(crop_ratio("fixed", (100, 50), Some(0.5)), Some(0.5));
    assert_eq!(crop_ratio("16:9", (100, 50), None), Some(16.0 / 9.0));
    assert_eq!(crop_ratio("junk", (100, 50), None), None);
    // Fitting 1:1 into a 4:1 box keeps the height and centers horizontally.
    let fitted = fit_crop_to_ratio(
        NormRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        1.0,
        (200, 100),
    );
    assert!((fitted.width - 0.5).abs() < 1e-9);
    assert!((fitted.height - 1.0).abs() < 1e-9);
    assert!((fitted.x - 0.25).abs() < 1e-9);
    // Moving clamps into the remaining space.
    let moved = move_crop(
        NormRect {
            x: 0.4,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        },
        0.4,
        -0.2,
    );
    assert!((moved.x - 0.5).abs() < 1e-9);
    assert!((moved.y - 0.0).abs() < 1e-9);
    // A corner resize with a locked 1:1 ratio keeps the square.
    let resized = resize_crop(
        NormRect {
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.2,
        },
        0.1,
        0.05,
        "se",
        Some(1.0),
        (100.0, 100.0),
    );
    assert!((resized.width - resized.height).abs() < 1e-9, "{resized:?}");
    assert_eq!(format_ratio(1920, 1080), "16:9");
    assert_eq!(format_ratio(7, 7), "1:1");
    // The crop tool's readouts use the pixel rect.
    let tool = CropTool::new((100, 50));
    assert_eq!(tool.size_label(), "76 x 38");
    assert_eq!(tool.ratio_label(), "2:1");
}

#[test]
fn a_crop_tool_round_trips_through_the_node_bitmap() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(8, 4)).ok_or("fixture node")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let surface = node_surface(&node).ok_or("surface")?;
    let rect = crop_rect_pixels(
        NormRect {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        },
        surface.width(),
        surface.height(),
    );
    let cropped = crop_surface_clamped(&surface, rect);
    let now = app.now_ms();
    assert!(app.write_node_surface(&id, &cropped, now));
    let updated = app.doc.node(&id).cloned().ok_or("updated")?;
    let meta = updated.metadata.as_ref().ok_or("metadata")?;
    assert_eq!(meta.natural_width, Some(4.0));
    assert_eq!(meta.natural_height, Some(4.0));
    assert_eq!(meta.thumbnail, None, "the stale thumbnail is cleared");
    let decoded = node_surface(&updated).ok_or("decoded")?;
    assert_eq!((decoded.width(), decoded.height()), (4, 4));
    assert_eq!(decoded.get(0, 0), [20, 20, 220, 255]);
    Ok(())
}

// ---- Mask -------------------------------------------------------------------

#[test]
fn a_rect_mask_clears_the_corners_and_keeps_the_center() {
    let source = red_solid(40, 40);
    let masked = masked_surface(
        &source,
        &MaskTool {
            shape: "rect".to_string(),
            feather: 0.0,
            invert: false,
        },
    );
    assert_eq!(masked.get(0, 0)[3], 0, "outside the mask is transparent");
    assert_eq!(masked.get(39, 39)[3], 0);
    let center = masked.get(20, 20);
    assert_eq!(center[3], 255, "inside the mask keeps its alpha");
    assert_eq!(center[0], 220, "the colour is untouched");
}

#[test]
fn an_inverted_mask_keeps_the_corners_and_clears_the_center() {
    let source = red_solid(40, 40);
    let masked = masked_surface(
        &source,
        &MaskTool {
            shape: "ellipse".to_string(),
            feather: 0.0,
            invert: true,
        },
    );
    assert_eq!(masked.get(0, 0)[3], 255, "the surround survives");
    assert_eq!(masked.get(20, 20)[3], 0, "the ellipse is cut out");
}

// ---- Split ------------------------------------------------------------------

#[test]
fn split_cuts_follow_the_grid_and_the_explicit_lines() {
    assert_eq!(split_cuts(&[], 100, 2), vec![0, 50, 100]);
    assert_eq!(split_cuts(&[], 100, 4), vec![0, 25, 50, 75, 100]);
    assert_eq!(split_cuts(&[0.25, 0.75], 100, 2), vec![0, 25, 75, 100]);
    // Out-of-bounds and duplicated lines are dropped.
    assert_eq!(split_cuts(&[0.0, 1.0, 0.5, 0.5], 100, 2), vec![0, 50, 100]);
    assert_eq!(grid_lines(4), vec![0.25, 0.5, 0.75]);
    assert_eq!(grid_lines(1), Vec::<f64>::new());
    assert!((largest_gap_spot(&[]) - 0.5).abs() < 1e-9);
    assert!((largest_gap_spot(&[0.2]) - 0.6).abs() < 1e-9);
    assert_eq!(clamp_grid(0.2), 1);
    assert_eq!(clamp_grid(99.0), SPLIT_MAX_GRID);
    assert_eq!(clamp_grid(f64::NAN), 1);
}

#[test]
fn splitting_a_synthetic_buffer_yields_the_expected_quadrants() {
    let source = split_surface(8, 8);
    let pieces = split_pieces(&source, &SplitTool::default());
    assert_eq!(pieces.len(), 4);
    let by_slot: Vec<(usize, usize, [u8; 4])> = pieces
        .iter()
        .map(|(row, column, rect)| {
            let piece = crop_surface_clamped(&source, *rect);
            assert_eq!((piece.width(), piece.height()), (4, 4));
            (*row, *column, piece.get(0, 0))
        })
        .collect();
    assert_eq!(by_slot[0], (0, 0, [220, 20, 20, 255]));
    assert_eq!(by_slot[1], (0, 1, [20, 20, 220, 255]));
    assert_eq!(by_slot[2], (1, 0, [220, 20, 20, 255]));
    assert_eq!(by_slot[3], (1, 1, [20, 20, 220, 255]));
    // The tool's line editors move the cuts.
    let mut tool = SplitTool::default();
    tool.add_line(true);
    assert_eq!(tool.rows, 3);
    assert_eq!(tool.horizontal_lines.len(), 2);
    tool.remove_line();
    assert_eq!(tool.rows, 2);
    tool.rows = 3;
    tool.reset_lines();
    assert_eq!(tool.horizontal_lines.len(), 2);
    assert_eq!(tool.total(), 6);
}

#[test]
fn splitting_a_node_creates_child_nodes_to_the_right() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(8, 8)).ok_or("fixture node")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let surface = node_surface(&node).ok_or("surface")?;
    let pieces = split_pieces(&surface, &SplitTool::default());
    let now = app.now_ms();
    let created = app.split_into_children(&node, &surface, &pieces, now);
    assert_eq!(created, 4);
    assert_eq!(app.doc.nodes().len(), 5);
    assert_eq!(
        app.doc.selection().len(),
        4,
        "the children become the selection"
    );
    let children: Vec<_> = app
        .doc
        .nodes()
        .iter()
        .filter(|node| node.id != id)
        .collect();
    assert!(children
        .iter()
        .all(|child| child.node_type == CanvasNodeType::Image));
    assert!(children
        .iter()
        .all(|child| child.position.x > node.position.x + node.width));
    assert!(children.iter().any(|child| child.title == "Fixture 1x1"));
    let first = children
        .iter()
        .find(|child| child.title == "Fixture 1x1")
        .copied()
        .ok_or("first piece")?;
    let decoded = node_surface(first).ok_or("decoded")?;
    assert_eq!(decoded.get(0, 0), [220, 20, 20, 255]);
    Ok(())
}

// ---- Resolution -------------------------------------------------------------

#[test]
fn resolution_resamples_the_buffer_and_records_the_target_size() {
    let source = split_surface(8, 4);
    let tool = ResolutionTool::new((8, 4));
    assert_eq!(tool.target, RESOLUTION_PRESETS[1]);
    assert_eq!(
        tool.resolved(),
        (2048, 1024),
        "the long edge drives both axes"
    );
    let mut tool = ResolutionTool {
        target: 64,
        algorithm: "nearest".to_string(),
        natural: (8, 4),
    };
    assert_eq!(
        tool.resolved(),
        (64, 32),
        "the minimum long edge is clamped in"
    );
    let resized = resample_surface(&source, 64, 32, upscale_algorithm(&tool.algorithm));
    assert_eq!((resized.width(), resized.height()), (64, 32));
    assert_eq!(resized.get(0, 0), [220, 20, 20, 255]);
    assert_eq!(resized.get(63, 31), [20, 20, 220, 255]);
    // Bilinear keeps a solid colour solid.
    tool.algorithm = "bilinear".to_string();
    let solid = {
        let mut surface = Surface::new(4, 4);
        surface.fill([10, 200, 30, 255]);
        surface
    };
    let resized = resample_surface(&solid, 16, 16, upscale_algorithm(&tool.algorithm));
    assert_eq!(resized.get(8, 8), [10, 200, 30, 255]);
    assert_eq!(upscale_algorithm("nonsense"), UpscaleAlgorithm::High);
    assert_eq!(upscale_algorithm("nearest"), UpscaleAlgorithm::Nearest);
}

#[test]
fn resolution_creates_a_derived_child_node() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(8, 4)).ok_or("fixture node")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let surface = node_surface(&node).ok_or("surface")?;
    let resized = resample_surface(&surface, 64, 32, UpscaleAlgorithm::High);
    let now = app.now_ms();
    let child = app
        .derived_image_node(&node, &resized, "Resized", now)
        .ok_or("child")?;
    let child_node = app.doc.node(&child).cloned().ok_or("child node")?;
    assert_eq!(child_node.title, "Fixture Resized");
    assert_eq!(
        (child_node.width, child_node.height),
        (64.0, 32.0),
        "the child fits the resampled size"
    );
    let meta = child_node.metadata.as_ref().ok_or("metadata")?;
    assert_eq!(meta.natural_width, Some(64.0));
    assert_eq!(
        meta.primary_image_id,
        meta.images.as_ref().map(|images| images[0].id.clone())
    );
    Ok(())
}

// ---- Analysis ---------------------------------------------------------------

#[test]
fn analysis_reads_the_palette_hash_and_a_bounded_suggestion() {
    let surface = dominant_surface(40, 20, 30);
    let analysis = analyze_surface(&surface, "1:1");
    assert_eq!(analysis.natural, (40, 20));
    assert_eq!(analysis.palette.len(), 2);
    assert_eq!(
        analysis.palette[0].hex(),
        "#dd1111",
        "the dominant colour ranks first"
    );
    assert_eq!(analysis.palette[0].count, 600);
    assert_eq!(analysis.hash.len(), 16);
    assert_ne!(analysis.hash, "0000000000000000");
    assert_eq!(analysis.suggestion.width, 20);
    assert_eq!(analysis.suggestion.height, 20);
    assert!((0..=20).contains(&analysis.suggestion.x));
    let metadata = analysis_rows(&analysis.metadata);
    assert_eq!(metadata.len(), 5);
    assert_eq!(metadata[0].value.as_str(), "40 x 20 px");
    let swatches = palette_swatches(&analysis.palette);
    assert_eq!(swatches.len(), 2);
    let rect = analysis.suggested_rect();
    assert!(rect.x >= 0.0 && rect.x + rect.width <= 1.0);
    assert!((rect.width - 0.5).abs() < 1e-9, "{rect:?}");
    // The suggestion crops the node bitmap to the requested ratio.
    let pixels = crop_rect_pixels(rect, 40, 20);
    let cropped = crop_surface_clamped(&surface, pixels);
    assert_eq!((cropped.width(), cropped.height()), (20, 20));
}

#[test]
fn analysis_ratios_all_produce_in_bounds_rectangles() {
    let surface = split_surface(64, 32);
    for ratio in ANALYSIS_RATIOS {
        let analysis = analyze_surface(&surface, ratio);
        let rect = analysis.suggested_rect();
        assert!(rect.x + rect.width <= 1.0 + 1e-9, "{ratio} {rect:?}");
        assert!(rect.y + rect.height <= 1.0 + 1e-9, "{ratio} {rect:?}");
        assert!(
            rect.width >= MIN_CROP && rect.height >= MIN_CROP,
            "{ratio} {rect:?}"
        );
        assert_eq!(analysis.ratio, ratio);
    }
}

// ---- Copy / download / image geometry ---------------------------------------

#[test]
fn download_writes_the_stored_bytes_with_the_mime_extension() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(4, 4)).ok_or("fixture node")?;
    let message = app
        .image_download_core(&id)
        .map_err(std::io::Error::other)?;
    assert!(message.starts_with("Downloaded "), "{message}");
    let path = app
        .store
        .export_path_for("Fixture")
        .with_extension(extension_for_mime("image/png"));
    let bytes = std::fs::read(&path)?;
    assert_eq!(&bytes[..4], b"\x89PNG");
    Ok(())
}

#[test]
fn copy_and_download_report_missing_content() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let node = CanvasNodeData {
        id: "empty-image".into(),
        node_type: CanvasNodeType::Image,
        title: "Empty".into(),
        position: open_canvas_core::node::Position::new(0.0, 0.0),
        width: 100.0,
        height: 100.0,
        metadata: Some(CanvasNodeMetadata::default()),
    };
    app.doc.nodes_mut().push(node);
    assert!(app.image_download_core("empty-image").is_err());
    assert!(app.image_copy_core("empty-image").is_err());
    assert!(app.image_download_core("nope").is_err());
    Ok(())
}

#[test]
fn image_node_geometry_is_locked_to_the_bitmap_ratio() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(4, 2)).ok_or("fixture node")?;
    {
        let node = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .ok_or("node")?;
        node.width = 400.0;
        node.height = 100.0;
        node.metadata
            .get_or_insert_with(CanvasNodeMetadata::default)
            .free_resize = Some(true);
    }
    assert!(app.normalize_image_node_geometry(&id));
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    assert!((node.height - 200.0).abs() < 1e-9, "{node:?}");
    assert_eq!(
        node.metadata.as_ref().and_then(|meta| meta.free_resize),
        None
    );
    assert_eq!(crate::canvas_content::media_fit(&node), "contain");
    assert!(!app.normalize_image_node_geometry(&id));
    Ok(())
}

#[test]
fn image_geometry_decodes_the_bitmap_when_metadata_is_legacy() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(4, 2)).ok_or("fixture node")?;
    {
        let node = app
            .doc
            .nodes_mut()
            .iter_mut()
            .find(|node| node.id == id)
            .ok_or("node")?;
        node.width = 400.0;
        node.height = 100.0;
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        meta.natural_width = Some(0.0);
        meta.natural_height = Some(0.0);
        meta.images = None;
        meta.primary_image_id = None;
        meta.free_resize = Some(true);
    }
    assert!(app.normalize_image_node_geometry(&id));
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    assert!((node.height - 200.0).abs() < 1e-9, "{node:?}");
    let meta = node.metadata.as_ref().ok_or("metadata")?;
    assert_eq!(meta.natural_width, Some(4.0));
    assert_eq!(meta.natural_height, Some(2.0));
    Ok(())
}

// ---- Stack gallery ----------------------------------------------------------

/// A stack node carrying `images` (the fixture the gallery actions mutate).
fn stack_node(app: &mut App, images: &[(String, Surface)], at: WorldPos) -> Option<String> {
    let now = app.now_ms();
    let id = app.doc.create_node(CanvasNodeType::ImageStack, at, now);
    let entries: Vec<_> = images
        .iter()
        .enumerate()
        .map(|(index, (_, surface))| {
            node_image_entry(&format!("stack-{id}-{index}"), surface, None)
        })
        .collect();
    let cover = entries.first()?.clone();
    let node = app.doc.nodes_mut().iter_mut().find(|node| node.id == id)?;
    let meta = node
        .metadata
        .get_or_insert_with(CanvasNodeMetadata::default);
    meta.status = Some(CanvasNodeStatus::Success);
    meta.natural_width = Some(cover.natural_width);
    meta.natural_height = Some(cover.natural_height);
    meta.bytes = Some(cover.bytes);
    meta.mime_type = Some(cover.mime_type.clone());
    meta.content = Some(cover.content.clone());
    meta.primary_image_id = Some(cover.id.clone());
    meta.count = Some(i64::try_from(entries.len()).unwrap_or(i64::MAX));
    meta.images = Some(entries);
    Some(id)
}

/// A stack node with three images; the middle one is errored.
fn stack_fixture(app: &mut App) -> Option<String> {
    let images = vec![
        ("one".to_string(), split_surface(4, 4)),
        ("two".to_string(), split_surface(6, 6)),
        ("three".to_string(), split_surface(8, 8)),
    ];
    let id = stack_node(app, &images, WorldPos::new(0.0, 0.0))?;
    if let Some(node) = app.doc.nodes_mut().iter_mut().find(|node| node.id == id) {
        if let Some(meta) = node.metadata.as_mut() {
            if let Some(images) = meta.images.as_mut() {
                images[1].status = CanvasNodeStatus::Error;
                images[1].error_details = Some("Provider failed".to_string());
            }
        }
    }
    Some(id)
}

#[test]
fn gallery_set_cover_moves_the_primary_image() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = stack_fixture(&mut app).ok_or("stack")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let images = stack_images(&node).to_vec();
    assert_eq!(images.len(), 3);
    let message = app
        .stack_action_core(&id, &images[2].id, "set-cover")
        .map_err(std::io::Error::other)?;
    assert_eq!(message, "Cover updated");
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let meta = node.metadata.as_ref().ok_or("metadata")?;
    assert_eq!(
        meta.primary_image_id.as_deref(),
        Some(images[2].id.as_str())
    );
    assert_eq!(meta.content.as_deref(), Some(images[2].content.as_str()));
    assert_eq!(meta.natural_width, Some(8.0));
    // The preview's cover follows the new primary image.
    let mut cache = crate::media::MediaCache::default();
    let preview = crate::canvas_content::preview_content(&node, &mut cache);
    assert_eq!(preview.rows.len(), 3);
    assert!(preview.rows[2].cover);
    assert!(!preview.rows[0].cover);
    assert_eq!(preview.rows[1].status.as_str(), "error");
    assert_eq!(preview.rows[1].error.as_str(), "Provider failed");
    assert!(preview.image.size().width > 0, "the cover decodes");
    Ok(())
}

#[test]
fn gallery_remove_take_out_retry_and_split_all_mutate_the_stack() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = stack_fixture(&mut app).ok_or("stack")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let images = stack_images(&node).to_vec();

    // Retry clears the errored entry.
    let message = app
        .stack_action_core(&id, &images[1].id, "retry")
        .map_err(std::io::Error::other)?;
    assert_eq!(message, "Retried the image");
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let recovered = stack_images(&node)
        .iter()
        .find(|image| image.id == images[1].id)
        .cloned()
        .ok_or("image")?;
    assert_eq!(recovered.status, CanvasNodeStatus::Success);
    assert_eq!(recovered.error_details, None);

    // Remove drops one entry and re-points the cover when it was removed.
    let message = app
        .stack_action_core(&id, &images[0].id, "remove")
        .map_err(std::io::Error::other)?;
    assert_eq!(message, "Removed the image from the stack");
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    assert_eq!(stack_images(&node).len(), 2);
    assert_eq!(
        node.metadata
            .as_ref()
            .and_then(|meta| meta.primary_image_id.clone()),
        Some(images[1].id.clone())
    );

    // Take out creates a sibling Image node.
    let before = app.doc.nodes().len();
    let message = app
        .stack_action_core(&id, &images[2].id, "take-out")
        .map_err(std::io::Error::other)?;
    assert_eq!(message, "Took the image out of the stack");
    assert_eq!(app.doc.nodes().len(), before + 1);
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    assert_eq!(stack_images(&node).len(), 1);

    // A single-image stack refuses to split.
    assert!(app.stack_action_core(&id, "", "split-all").is_err());

    // With two entries split-all creates one node per image.
    let two = vec![
        ("a".to_string(), split_surface(4, 4)),
        ("b".to_string(), split_surface(4, 4)),
    ];
    let second = stack_node(&mut app, &two, WorldPos::new(0.0, 400.0)).ok_or("second stack")?;
    let before = app.doc.nodes().len();
    let message = app
        .stack_action_core(&second, "", "split-all")
        .map_err(std::io::Error::other)?;
    assert_eq!(message, "Split 2 images out of the stack");
    assert_eq!(app.doc.nodes().len(), before + 2);
    assert_eq!(app.doc.selection().len(), 2);
    // Unknown actions and nodes are refused.
    assert!(app.stack_action_core(&second, "x", "nonsense").is_err());
    assert!(app.stack_action_core("missing", "x", "remove").is_err());
    Ok(())
}

#[test]
fn gallery_download_writes_one_image_and_reports_unknown_entries() -> TestResult {
    let (mut app, dir) = temp_app()?;
    let id = stack_fixture(&mut app).ok_or("stack")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let images = stack_images(&node).to_vec();
    let message = app
        .stack_action_core(&id, &images[1].id, "download")
        .map_err(std::io::Error::other)?;
    assert!(message.starts_with("Downloaded "), "{message}");
    let downloaded = std::fs::read_dir(dir.path().join("exports"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .count();
    assert!(
        downloaded >= 1,
        "the stack image landed in the exports folder"
    );
    assert!(app
        .stack_action_core(&id, "missing-image", "download")
        .is_err());
    assert!(app
        .stack_action_core(&id, &images[0].id, "set-cover")
        .is_ok());
    assert!(app
        .stack_action_core(&id, "missing-image", "set-cover")
        .is_err());
    Ok(())
}

// ---- Preview content --------------------------------------------------------

#[test]
fn preview_content_of_an_image_carries_the_bitmap_and_the_info_line() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let id = image_node(&mut app, &split_surface(4, 4)).ok_or("fixture node")?;
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let mut cache = crate::media::MediaCache::default();
    let preview = crate::canvas_content::preview_content(&node, &mut cache);
    assert_eq!(
        preview.image.size().width,
        4,
        "the bitmap decodes for the modal"
    );
    assert!(
        preview.rows.is_empty(),
        "a single image has no gallery rows"
    );
    assert!(
        preview.info.starts_with("Fixture \u{00b7} 4 x 4"),
        "{}",
        preview.info
    );
    Ok(())
}

#[test]
fn preview_content_of_an_empty_node_stays_unavailable() -> TestResult {
    let (mut app, _dir) = temp_app()?;
    let now = app.now_ms();
    let id = app
        .doc
        .create_node(CanvasNodeType::Image, WorldPos::new(0.0, 0.0), now);
    let node = app.doc.node(&id).cloned().ok_or("node")?;
    let mut cache = crate::media::MediaCache::default();
    let preview = crate::canvas_content::preview_content(&node, &mut cache);
    assert_eq!(preview.image.size().width, 0, "no bitmap to paint");
    Ok(())
}

// ---- Dialog state -----------------------------------------------------------

#[test]
fn the_crop_tool_state_tracks_the_pointer_drag() {
    let mut crop = CropTool::new((100, 50));
    // A move drag translates the rect and clamps it.
    crop.drag = Some(("move".to_string(), 0.5, 0.5));
    let moved = move_crop(crop.rect, 0.4, 0.0);
    assert!((moved.x - (1.0 - moved.width)).abs() < 1e-9);
    // The corner handle resize keeps the rect inside the unit square.
    let resized = resize_crop(crop.rect, 0.9, 0.9, "se", None, (100.0, 50.0));
    assert!(resized.x + resized.width <= 1.0 + 1e-9);
    assert!(resized.y + resized.height <= 1.0 + 1e-9);
    // Picking a chip re-fits the rect and records the fixed ratio.
    crop.pick_ratio("fixed");
    assert!(crop.fixed.is_some());
    crop.pick_ratio("1:1");
    assert!(
        (crop.rect.width - crop.rect.height * 0.5).abs() < 1e-9,
        "1:1 in a 2:1 image: {crop:?}"
    );
    assert_eq!(crop.resolved_ratio(), Some(1.0));
    crop.pick_ratio("nonsense");
    assert_eq!(crop.ratio, "1:1", "unknown chips are ignored");
    // The press position picks the drag mode: inside is a move, a corner is a
    // resize, outside starts no drag.
    let rect = DEFAULT_CROP;
    assert_eq!(
        crop_handle_at(rect, 0.5, 0.5, CROP_HANDLE_TOLERANCE),
        "move"
    );
    assert_eq!(
        crop_handle_at(rect, rect.x, rect.y, CROP_HANDLE_TOLERANCE),
        "nw"
    );
    assert_eq!(
        crop_handle_at(rect, rect.x + rect.width, rect.y, CROP_HANDLE_TOLERANCE),
        "ne"
    );
    assert_eq!(
        crop_handle_at(rect, rect.x, rect.y + rect.height, CROP_HANDLE_TOLERANCE),
        "sw"
    );
    assert_eq!(crop_handle_at(rect, 0.99, 0.99, CROP_HANDLE_TOLERANCE), "");
}

#[test]
fn crop_field_edits_accept_percent_and_fraction() {
    let rect = DEFAULT_CROP;
    let edited = crop_field(rect, "x", "25%").unwrap_or(rect);
    assert!((edited.x - 0.25).abs() < 1e-9);
    let edited = crop_field(rect, "width", "0.5").unwrap_or(rect);
    assert!((edited.width - 0.5).abs() < 1e-9);
    let edited = crop_field(rect, "height", "0").unwrap_or(rect);
    assert!(
        (edited.height - MIN_CROP).abs() < 1e-9,
        "the minimum is enforced"
    );
    assert!(crop_field(rect, "junk", "1").is_none());
    assert!(crop_field(rect, "x", "abc").is_none());
}

#[test]
fn data_url_bytes_reads_the_payload_and_extension_for_mime_maps_formats() -> TestResult {
    let surface = split_surface(2, 2);
    let (url, _bytes) = surface_png(&surface).map_err(std::io::Error::other)?;
    let bytes = data_url_bytes(&url).ok_or("payload").unwrap_or_default();
    assert_eq!(&bytes[..4], b"\x89PNG");
    assert!(data_url_bytes("!!!").is_none());
    assert_eq!(extension_for_mime("image/jpeg"), "jpg");
    assert_eq!(extension_for_mime("image/webp"), "webp");
    assert_eq!(extension_for_mime("image/png"), "png");
    assert_eq!(extension_for_mime("application/octet-stream"), "png");
    Ok(())
}
