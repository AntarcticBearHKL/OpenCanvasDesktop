//! Argument parsing. Hand-rolled (no parser dependency): the surface is three
//! subcommands with a handful of repeatable options, and every malformed input
//! must become a clean usage error, never a panic.

use std::path::PathBuf;

use crate::project::output_kind;
use crate::{
    BATCH_HELP, BATCH_HINT, CONVERT_HELP, CONVERT_HINT, INSPECT_HELP, INSPECT_HINT, TOP_HELP,
    TOP_HINT,
};

/// What the parsed command line asks for.
#[derive(Debug)]
pub enum Action {
    Run(Command),
    Help(&'static str),
    Version,
}

/// One subcommand with its validated arguments.
#[derive(Debug)]
pub enum Command {
    Convert(ConvertArgs),
    Inspect(InspectArgs),
    Batch(BatchArgs),
}

#[derive(Debug)]
pub struct ConvertArgs {
    pub input: PathBuf,
    /// Filter/adjustment ids in application order.
    pub filters: Vec<String>,
    /// Raw `key=value` overrides, shared by every filter that knows the key.
    pub params: Vec<String>,
    pub output: Option<PathBuf>,
}

#[derive(Debug)]
pub struct InspectArgs {
    pub file: PathBuf,
    pub json: bool,
}

#[derive(Debug)]
pub struct BatchArgs {
    pub directory: PathBuf,
    pub filters: Vec<String>,
    pub params: Vec<String>,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct UsageError {
    pub message: String,
    pub hint: &'static str,
}

impl UsageError {
    fn new(message: impl Into<String>, hint: &'static str) -> Self {
        Self {
            message: message.into(),
            hint,
        }
    }
}

/// Parses the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Action, UsageError> {
    let Some(first) = args.first() else {
        return Err(UsageError::new("no command given", TOP_HINT));
    };
    match first.as_str() {
        "-h" | "--help" => Ok(Action::Help(TOP_HELP)),
        "-V" | "--version" => Ok(Action::Version),
        "convert" => parse_convert(&args[1..]),
        "inspect" => parse_inspect(&args[1..]),
        "batch" => parse_batch(&args[1..]),
        other if other.starts_with('-') => Err(UsageError::new(
            format!("unknown option \"{other}\""),
            TOP_HINT,
        )),
        other => Err(UsageError::new(
            format!("unknown command \"{other}\""),
            TOP_HINT,
        )),
    }
}

fn parse_convert(args: &[String]) -> Result<Action, UsageError> {
    let mut input: Option<PathBuf> = None;
    let mut filters = Vec::new();
    let mut params = Vec::new();
    let mut output = None;
    let mut index = 0;
    let mut positional_only = false;
    while index < args.len() {
        let argument = args[index].as_str();
        if !positional_only && argument == "--" {
            positional_only = true;
            index += 1;
            continue;
        }
        if !positional_only {
            if argument == "-h" || argument == "--help" {
                return Ok(Action::Help(CONVERT_HELP));
            }
            if argument == "-o" || argument == "--output" {
                output = Some(PathBuf::from(next_value(
                    args,
                    &mut index,
                    argument,
                    CONVERT_HINT,
                )?));
                continue;
            }
            if let Some(value) = argument.strip_prefix("--output=") {
                output = Some(PathBuf::from(non_empty(value, "--output", CONVERT_HINT)?));
                index += 1;
                continue;
            }
            if argument == "--filter" {
                filters.push(next_value(args, &mut index, argument, CONVERT_HINT)?);
                continue;
            }
            if let Some(value) = argument.strip_prefix("--filter=") {
                filters.push(non_empty(value, "--filter", CONVERT_HINT)?);
                index += 1;
                continue;
            }
            if argument == "--param" {
                params.push(next_value(args, &mut index, argument, CONVERT_HINT)?);
                continue;
            }
            if let Some(value) = argument.strip_prefix("--param=") {
                params.push(non_empty(value, "--param", CONVERT_HINT)?);
                index += 1;
                continue;
            }
            if argument.starts_with('-') && argument != "-" {
                return Err(UsageError::new(
                    format!("unknown option \"{argument}\""),
                    CONVERT_HINT,
                ));
            }
        }
        if input.is_some() {
            return Err(UsageError::new(
                format!("unexpected argument \"{argument}\""),
                CONVERT_HINT,
            ));
        }
        input = Some(PathBuf::from(argument));
        index += 1;
    }
    let Some(input) = input else {
        return Err(UsageError::new("missing <input>", CONVERT_HINT));
    };
    if let Some(path) = &output {
        output_kind(path).map_err(|message| UsageError::new(message, CONVERT_HINT))?;
    }
    Ok(Action::Run(Command::Convert(ConvertArgs {
        input,
        filters,
        params,
        output,
    })))
}

fn parse_inspect(args: &[String]) -> Result<Action, UsageError> {
    let mut file: Option<PathBuf> = None;
    let mut json = false;
    let mut index = 0;
    let mut positional_only = false;
    while index < args.len() {
        let argument = args[index].as_str();
        if !positional_only && argument == "--" {
            positional_only = true;
            index += 1;
            continue;
        }
        if !positional_only {
            if argument == "-h" || argument == "--help" {
                return Ok(Action::Help(INSPECT_HELP));
            }
            if argument == "--json" {
                json = true;
                index += 1;
                continue;
            }
            if argument.starts_with('-') && argument != "-" {
                return Err(UsageError::new(
                    format!("unknown option \"{argument}\""),
                    INSPECT_HINT,
                ));
            }
        }
        if file.is_some() {
            return Err(UsageError::new(
                format!("unexpected argument \"{argument}\""),
                INSPECT_HINT,
            ));
        }
        file = Some(PathBuf::from(argument));
        index += 1;
    }
    let Some(file) = file else {
        return Err(UsageError::new("missing <file>", INSPECT_HINT));
    };
    Ok(Action::Run(Command::Inspect(InspectArgs { file, json })))
}

fn parse_batch(args: &[String]) -> Result<Action, UsageError> {
    let mut directory: Option<PathBuf> = None;
    let mut filters = Vec::new();
    let mut params = Vec::new();
    let mut output_dir = None;
    let mut index = 0;
    let mut positional_only = false;
    while index < args.len() {
        let argument = args[index].as_str();
        if !positional_only && argument == "--" {
            positional_only = true;
            index += 1;
            continue;
        }
        if !positional_only {
            if argument == "-h" || argument == "--help" {
                return Ok(Action::Help(BATCH_HELP));
            }
            if argument == "-o" || argument == "--output-dir" {
                output_dir = Some(PathBuf::from(next_value(
                    args, &mut index, argument, BATCH_HINT,
                )?));
                continue;
            }
            if let Some(value) = argument.strip_prefix("--output-dir=") {
                output_dir = Some(PathBuf::from(non_empty(value, "--output-dir", BATCH_HINT)?));
                index += 1;
                continue;
            }
            if argument == "--filter" {
                filters.push(next_value(args, &mut index, argument, BATCH_HINT)?);
                continue;
            }
            if let Some(value) = argument.strip_prefix("--filter=") {
                filters.push(non_empty(value, "--filter", BATCH_HINT)?);
                index += 1;
                continue;
            }
            if argument == "--param" {
                params.push(next_value(args, &mut index, argument, BATCH_HINT)?);
                continue;
            }
            if let Some(value) = argument.strip_prefix("--param=") {
                params.push(non_empty(value, "--param", BATCH_HINT)?);
                index += 1;
                continue;
            }
            if argument.starts_with('-') && argument != "-" {
                return Err(UsageError::new(
                    format!("unknown option \"{argument}\""),
                    BATCH_HINT,
                ));
            }
        }
        if directory.is_some() {
            return Err(UsageError::new(
                format!("unexpected argument \"{argument}\""),
                BATCH_HINT,
            ));
        }
        directory = Some(PathBuf::from(argument));
        index += 1;
    }
    let Some(directory) = directory else {
        return Err(UsageError::new("missing <directory>", BATCH_HINT));
    };
    if filters.is_empty() {
        return Err(UsageError::new(
            "batch needs at least one --filter <id>",
            BATCH_HINT,
        ));
    }
    Ok(Action::Run(Command::Batch(BatchArgs {
        directory,
        filters,
        params,
        output_dir,
    })))
}

/// The value of an option that takes one, advancing past it.
fn next_value(
    args: &[String],
    index: &mut usize,
    option: &str,
    hint: &'static str,
) -> Result<String, UsageError> {
    let Some(value) = args.get(*index + 1) else {
        return Err(UsageError::new(format!("{option} needs a value"), hint));
    };
    *index += 2;
    Ok(value.clone())
}

fn non_empty(value: &str, option: &str, hint: &'static str) -> Result<String, UsageError> {
    if value.is_empty() {
        Err(UsageError::new(format!("{option} needs a value"), hint))
    } else {
        Ok(value.to_string())
    }
}
