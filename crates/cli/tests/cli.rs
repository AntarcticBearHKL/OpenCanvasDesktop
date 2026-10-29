//! End-to-end tests of the CLI surface in a temporary directory. They drive
//! `run` (parsing + exit codes) and the public report helpers (whose output is
//! otherwise only written to stdout).

use std::collections::HashMap;
use std::path::Path;

use open_canvas_ps::doc::BoardResolution;
use open_canvas_ps::{
    base_layer, decode_rgba, encode_png, export_project, Bounds, CanvasPsLayerKind, ImageProject,
    Surface,
};

fn arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn png_bytes(width: usize, height: usize, rgba: [u8; 4]) -> Result<Vec<u8>, std::io::Error> {
    let mut surface = Surface::new(width, height);
    surface.fill(rgba);
    encode_png(&surface).map_err(std::io::Error::other)
}

/// A 1024x1024 one-layer board: the 1:1 1k preset, so export scale is 1.
fn sample_project() -> ImageProject {
    let mut layer = base_layer(
        "l1".to_string(),
        "Base",
        CanvasPsLayerKind::Pixel,
        Bounds::new(0.0, 0.0, 1024.0, 1024.0),
    );
    layer.storage_key = Some("image:l1".to_string());
    ImageProject {
        id: "img-1".to_string(),
        title: "Board".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        group_id: String::new(),
        width: 1024.0,
        height: 1024.0,
        board_ratio: "1:1".to_string(),
        board_resolution: BoardResolution::K1,
        board_background: "transparent".to_string(),
        board_background_opacity: 1.0,
        board_layers: vec![layer],
        board_paths: None,
        board_alpha_channels: None,
        board_channel_visibility: None,
    }
}

#[test]
fn convert_round_trip_changes_pixels() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let input = dir.path().join("art.png");
    std::fs::write(&input, png_bytes(1024, 1024, [10, 20, 30, 255])?)?;
    let output = dir.path().join("art-out.png");

    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&input),
        "--filter".to_string(),
        "invert".to_string(),
        "-o".to_string(),
        arg(&output),
    ]);
    assert_eq!(code, 0, "convert succeeds");

    let bytes = std::fs::read(&output)?;
    let back = decode_rgba(&bytes)?;
    assert_eq!((back.width(), back.height()), (1024, 1024));
    assert_eq!(
        back.get(512, 512),
        [245, 235, 225, 255],
        "invert changed the pixel and kept alpha"
    );
    Ok(())
}

#[test]
fn convert_writes_a_psd_then_reads_it_back() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let input = dir.path().join("art.png");
    std::fs::write(&input, png_bytes(1024, 1024, [1, 2, 3, 255])?)?;
    let psd = dir.path().join("art.psd");

    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&input),
        "--filter".to_string(),
        "brightness-contrast".to_string(),
        "--param".to_string(),
        "brightness=50".to_string(),
        "-o".to_string(),
        arg(&psd),
    ]);
    assert_eq!(code, 0, "convert to PSD succeeds");
    assert_eq!(&std::fs::read(&psd)?[..4], b"8BPS");

    let text = open_canvas_cli::text_report(&psd).map_err(std::io::Error::other)?;
    assert!(text.contains("format:     psd"), "{text}");
    assert!(text.contains("size:       1024 x 1024"), "{text}");
    assert!(text.contains("layers:     1"), "{text}");
    assert!(text.contains("Base") || text.contains("art"), "{text}");

    // The filtered PSD converts back to a PNG.
    let png = dir.path().join("back.png");
    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&psd),
        "-o".to_string(),
        arg(&png),
    ]);
    assert_eq!(code, 0, "PSD -> PNG succeeds");
    let back = decode_rgba(&std::fs::read(&png)?)?;
    assert_eq!((back.width(), back.height()), (1024, 1024));
    Ok(())
}

#[test]
fn inspect_reports_a_psd_our_exporter_wrote() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let path = dir.path().join("sample.psd");
    let mut bitmaps = HashMap::new();
    let mut bitmap = Surface::new(1024, 1024);
    bitmap.fill([200, 10, 60, 255]);
    bitmaps.insert("image:l1".to_string(), bitmap);
    std::fs::write(&path, export_project(&sample_project(), &bitmaps)?)?;

    let text = open_canvas_cli::text_report(&path).map_err(std::io::Error::other)?;
    assert!(text.contains("format:     psd"), "{text}");
    assert!(text.contains("color mode: rgb"), "{text}");
    assert!(text.contains("depth:      8 bit"), "{text}");
    assert!(text.contains("size:       1024 x 1024"), "{text}");
    assert!(text.contains("layers:     1"), "{text}");
    assert!(text.contains("Base"), "{text}");
    assert!(text.contains("visible"), "{text}");
    assert!(text.contains("opacity=1.00 blend=normal"), "{text}");

    let json = open_canvas_cli::json_report(&path).map_err(std::io::Error::other)?;
    let value: serde_json::Value = serde_json::from_str(&json)?;
    assert_eq!(value["format"], "psd");
    assert_eq!(value["width"], 1024);
    assert_eq!(value["depth"], 8);
    assert_eq!(value["layers"][0]["name"], "Base");
    assert_eq!(value["layers"][0]["visible"], true);

    // The subcommand itself also succeeds.
    assert_eq!(
        open_canvas_cli::run(vec![
            "inspect".to_string(),
            arg(&path),
            "--json".to_string()
        ]),
        0
    );
    Ok(())
}

#[test]
fn batch_processes_two_files_with_the_same_basename() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let source = dir.path().join("src");
    std::fs::create_dir_all(&source)?;
    std::fs::write(source.join("a.png"), png_bytes(1024, 1024, [0, 0, 0, 255])?)?;
    std::fs::write(
        source.join("b.png"),
        png_bytes(1024, 1024, [255, 255, 255, 255])?,
    )?;
    let output = dir.path().join("out");

    let code = open_canvas_cli::run(vec![
        "batch".to_string(),
        arg(&source),
        "--filter".to_string(),
        "invert".to_string(),
        "-o".to_string(),
        arg(&output),
    ]);
    assert_eq!(code, 0, "batch succeeds");

    let a = decode_rgba(&std::fs::read(output.join("a.png"))?)?;
    let b = decode_rgba(&std::fs::read(output.join("b.png"))?)?;
    assert_eq!(a.get(0, 0), [255, 255, 255, 255]);
    assert_eq!(b.get(0, 0), [0, 0, 0, 255]);
    Ok(())
}

#[test]
fn bad_inputs_return_clean_errors() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let missing = dir.path().join("nope.png");

    // Unknown filter id: usage error before any file access.
    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&missing),
        "--filter".to_string(),
        "no-such-filter".to_string(),
    ]);
    assert_eq!(code, 2, "bad filter id is a usage error");

    // Unknown parameter for the chain: also usage.
    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&missing),
        "--filter".to_string(),
        "invert".to_string(),
        "--param".to_string(),
        "no-such-param=1".to_string(),
    ]);
    assert_eq!(code, 2, "bad parameter is a usage error");

    // Unsupported output extension: usage.
    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&missing),
        "-o".to_string(),
        arg(&dir.path().join("out.gif")),
    ]);
    assert_eq!(code, 2, "bad output extension is a usage error");

    // Missing input file: operation failure, not a panic.
    let code = open_canvas_cli::run(vec![
        "convert".to_string(),
        arg(&missing),
        "--filter".to_string(),
        "invert".to_string(),
    ]);
    assert_eq!(code, 1, "missing file is a failure");

    let code = open_canvas_cli::run(vec!["inspect".to_string(), arg(&missing)]);
    assert_eq!(code, 1, "inspect of a missing file is a failure");

    // An empty batch directory is a failure, and a missing --filter is usage.
    let code = open_canvas_cli::run(vec![
        "batch".to_string(),
        arg(dir.path()),
        "--filter".to_string(),
        "invert".to_string(),
    ]);
    assert_eq!(code, 1, "empty directory is a failure");
    let code = open_canvas_cli::run(vec!["batch".to_string(), arg(dir.path())]);
    assert_eq!(code, 2, "missing --filter is a usage error");
    Ok(())
}

#[test]
fn help_is_available_for_every_subcommand() {
    assert_eq!(open_canvas_cli::run(vec!["convert", "--help"]), 0);
    assert_eq!(open_canvas_cli::run(vec!["inspect", "--help"]), 0);
    assert_eq!(open_canvas_cli::run(vec!["batch", "--help"]), 0);
}
