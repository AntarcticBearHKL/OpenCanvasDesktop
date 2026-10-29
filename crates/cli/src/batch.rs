//! `batch`: apply one filter chain to every supported image in a directory.

use std::path::{Path, PathBuf};

use crate::args::BatchArgs;
use crate::ops::{self, Op};
use crate::project;
use crate::CliError;

/// Runs `batch`. Individual file failures are reported per file and make the
/// exit code 1; a bad chain or unreadable directory fails before any work.
pub fn run(args: &BatchArgs) -> Result<(), CliError> {
    let chain = ops::resolve(&args.filters, &args.params).map_err(CliError::usage)?;
    let files = supported_files(&args.directory)?;
    if files.is_empty() {
        return Err(CliError::failure(format!(
            "no supported images in {}",
            args.directory.display()
        )));
    }
    let output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| args.directory.join("out"));
    std::fs::create_dir_all(&output_dir).map_err(|error| {
        CliError::failure(format!("cannot create {}: {error}", output_dir.display()))
    })?;
    let mut failed = 0_usize;
    for path in &files {
        match process(path, &chain, &output_dir) {
            Ok(output) => println!("{} -> {}", path.display(), output.display()),
            Err(message) => {
                eprintln!("failed {}: {message}", path.display());
                failed += 1;
            }
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(CliError::failure(format!(
            "{failed} of {} file(s) failed",
            files.len()
        )))
    }
}

fn process(path: &Path, chain: &[Op], output_dir: &Path) -> Result<PathBuf, String> {
    let mut loaded = project::load(path)?;
    ops::apply_all(&mut loaded, chain)?;
    let extension = if loaded.format == "psd" || loaded.format == "psb" {
        "psd"
    } else {
        "png"
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    let output = output_dir.join(format!("{stem}.{extension}"));
    if output.as_path() == path {
        return Err("refusing to overwrite the input".to_string());
    }
    project::write(&loaded.project, &loaded.bitmaps, &output)?;
    Ok(output)
}

/// Regular files directly inside `dir` with a supported extension, in a
/// stable case-insensitive name order.
fn supported_files(dir: &Path) -> Result<Vec<PathBuf>, CliError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| CliError::failure(format!("cannot read {}: {error}", dir.display())))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::failure(format!("cannot read {}: {error}", dir.display()))
        })?;
        let path = entry.path();
        if path.is_file() && supported_extension(&path) {
            files.push(path);
        }
    }
    files.sort_by(|left, right| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
    });
    Ok(files)
}

fn supported_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "bmp" | "psd" | "psb")
    )
}
