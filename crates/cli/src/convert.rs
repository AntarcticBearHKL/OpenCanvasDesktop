//! `convert`: one input, an optional filter chain, one output.

use std::path::{Path, PathBuf};

use crate::args::ConvertArgs;
use crate::ops::{self, Op};
use crate::project;
use crate::CliError;

/// Runs `convert`. A bad id or parameter is a usage error (exit 2); a file or
/// encode failure is an operation failure (exit 1).
pub fn run(args: &ConvertArgs) -> Result<(), CliError> {
    let chain = resolve(&args.filters, &args.params)?;
    let mut loaded = project::load(&args.input).map_err(CliError::failure)?;
    apply(&mut loaded, &chain)?;
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output(&args.input));
    project::write(&loaded.project, &loaded.bitmaps, &output).map_err(CliError::failure)?;
    println!("Wrote {}", output.display());
    Ok(())
}

/// Resolves a filter chain, reporting ids/params as usage errors.
pub fn resolve(filters: &[String], params: &[String]) -> Result<Vec<Op>, CliError> {
    ops::resolve(filters, params).map_err(CliError::usage)
}

/// Applies a chain to every raster layer of `loaded`.
pub fn apply(loaded: &mut project::Loaded, chain: &[Op]) -> Result<(), CliError> {
    ops::apply_all(loaded, chain)
        .map(|_applied| ())
        .map_err(CliError::failure)
}

/// `art.png` -> `art-out.png` beside the input.
fn default_output(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    input.with_file_name(format!("{stem}-out.png"))
}
