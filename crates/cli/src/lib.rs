//! Headless CLI for OpenCanvas image tooling.
//!
//! The crate is split into a library (`run`) and a thin binary so argument
//! parsing, exit codes and the conversions are testable without spawning a
//! process. `convert` and `batch` load files into an `ImageProject`, apply the
//! filter/adjustment ids from `open-canvas-ps` and write PNG or PSD through the
//! same pure mapping the desktop app uses; `inspect` reports either a PSD's raw
//! layers or the project summary of a raster image.

mod args;
mod batch;
mod convert;
mod inspect;
mod ops;
mod project;

pub use inspect::{json_report, text_report};

use args::{Action, Command};

/// `cargo`-style top-level help.
pub const TOP_HELP: &str = "\
open-canvas-cli - headless image and PSD tooling for OpenCanvas

Usage:
  open-canvas-cli <command> [options]
  open-canvas-cli --help
  open-canvas-cli --version

Commands:
  convert  Load an image, apply filters/adjustments in order, write PNG or PSD
  inspect  Show a PSD's layers, or the project summary of a raster image
  batch    Apply the same filter chain to every supported image in a directory

Run `open-canvas-cli <command> --help` for the options of a command.

Exit codes:
  0  success
  1  the operation failed
  2  bad usage
";

/// `convert --help`.
pub const CONVERT_HELP: &str = "\
Usage: open-canvas-cli convert <input> [--filter <id>]... [--param <key=value>]... [-o <output>]

Loads PNG, JPEG, BMP or PSD into an ImageProject, applies the filters and
adjustments in order to every raster layer, then writes a PNG (board composite)
or a PSD (layers plus merged composite), chosen by the output extension.

Arguments:
  <input>                PNG, JPEG, BMP or PSD file

Options:
  --filter <id>          Filter or adjustment id; repeat to chain them
  --param <key=value>    Override a parameter of the filters that use it;
                         a number (8), a number list (0,0,255,255) or text
  -o, --output <path>    Output file; .png or .psd (default: <name>-out.png)
  -h, --help             Show this help

Filter ids:
  gaussian-blur box-blur motion-blur radial-blur tilt-shift iris-blur
  sharpen unsharp-mask add-noise despeckle dust-scratches pixelate
  crystallize emboss find-edges solarize clouds lens-flare ripple pinch
  spherize liquify
Adjustment ids:
  brightness-contrast levels curves exposure vibrance hue-saturation
  color-balance black-white photo-filter channel-mixer gradient-map invert
  posterize threshold selective-color

Raster inputs take the studio board preset matching their aspect ratio (exactly
when the image already is a preset size, e.g. 1024x1024), so the PNG output is
that board's export size.
";

/// `inspect --help`.
pub const INSPECT_HELP: &str = "\
Usage: open-canvas-cli inspect <file> [--json]

Prints size, colour mode and layers. A PSD is read as raw layer records (name,
visibility, opacity, blend mode, group nesting); PNG/JPEG/BMP are loaded into
an ImageProject and reported as a one-layer project summary.

Arguments:
  <file>       PNG, JPEG, BMP or PSD file

Options:
  --json       Print machine-readable JSON
  -h, --help   Show this help
";

/// `batch --help`.
pub const BATCH_HELP: &str = "\
Usage: open-canvas-cli batch <directory> --filter <id> [--filter <id>]... [--param <key=value>]... [-o <outdir>]

Applies the filter chain to every supported image directly inside <directory>
(non-recursive). Results keep the input basename: PSD inputs stay PSD, raster
inputs become PNG.

Arguments:
  <directory>             Directory with PNG, JPEG, BMP or PSD files

Options:
  --filter <id>           Filter or adjustment id (at least one)
  --param <key=value>     Override a parameter of the filters that use it
  -o, --output-dir <dir>  Where to write results (default: <directory>/out)
  -h, --help              Show this help
";

const TOP_HINT: &str = "Run 'open-canvas-cli --help' for usage.";
const CONVERT_HINT: &str = "Run 'open-canvas-cli convert --help' for usage.";
const INSPECT_HINT: &str = "Run 'open-canvas-cli inspect --help' for usage.";
const BATCH_HINT: &str = "Run 'open-canvas-cli batch --help' for usage.";

/// A runtime failure (`code` 1) or a usage failure (`code` 2).
#[derive(Debug)]
pub struct CliError {
    pub message: String,
    pub code: i32,
}

impl CliError {
    /// Bad invocation: exit code 2.
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }

    /// Failed operation: exit code 1.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }
}

/// Runs the CLI with the arguments after the program name and returns the
/// process exit code (0 ok, 1 failure, 2 usage). Never panics on bad input.
pub fn run<I, S>(args: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    match args::parse(&args) {
        Ok(Action::Help(help)) => {
            print!("{help}");
            0
        }
        Ok(Action::Version) => {
            println!("open-canvas-cli {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(Action::Run(command)) => match dispatch(command) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("error: {}", error.message);
                error.code
            }
        },
        Err(usage) => {
            eprintln!("error: {}", usage.message);
            eprintln!("{}", usage.hint);
            2
        }
    }
}

fn dispatch(command: Command) -> Result<(), CliError> {
    match command {
        Command::Convert(arguments) => convert::run(&arguments),
        Command::Inspect(arguments) => inspect::run(&arguments),
        Command::Batch(arguments) => batch::run(&arguments),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_help_and_version_succeed() {
        assert_eq!(run(["--help"]), 0);
        assert_eq!(run(["-h"]), 0);
        assert_eq!(run(["--version"]), 0);
    }

    #[test]
    fn no_command_is_a_usage_error() {
        assert_eq!(run(std::iter::empty::<&str>()), 2);
        assert_eq!(run(["frobnicate"]), 2);
    }
}
