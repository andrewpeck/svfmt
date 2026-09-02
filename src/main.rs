//! `svfmt`: align and indent (System)Verilog sources. Only whitespace is
//! ever changed.

use clap::Parser;
use similar::TextDiff;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use svfmt::{Config, FormatOptions, find_config, format_text, iter_sources, read_config};

const GREEN: &str = "32";
const YELLOW: &str = "33";

const EPILOG: &str = "Paths to skip are read from the first sv-format.toml, .sv-format.toml, or \
pyproject.toml found in the working directory or one of its parents, e.g.\n\n  \
[tool.sv-format]\n  exclude = [\"Top/*/registers.sv\", \"vendor\"]\n\n\
Patterns are relative to that file. One containing a \"/\" matches the path (or a \
parent directory of it); one without matches any single path component.";

/// Align and indent (System)Verilog sources. Only whitespace is ever changed.
#[derive(Parser)]
#[command(name = "svfmt", version, after_help = EPILOG, arg_required_else_help = true)]
struct Args {
    /// files or directories to format ("-" for stdin)
    paths: Vec<PathBuf>,

    /// exit non-zero if any file would change; write nothing
    #[arg(long)]
    check: bool,

    /// print a unified diff instead of writing
    #[arg(long)]
    diff: bool,

    /// write the formatted result to stdout instead of editing the file (one file at a time)
    #[arg(long)]
    stdout: bool,

    /// skip the indentation pass
    #[arg(long = "align-only")]
    align_only: bool,

    /// skip the alignment pass
    #[arg(long = "indent-only")]
    indent_only: bool,

    /// spaces per indent level
    #[arg(long, default_value_t = 2, value_name = "N")]
    indent: usize,

    /// do not report reformatted files
    #[arg(short = 'q', long)]
    quiet: bool,

    /// read settings from FILE instead of searching
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// skip paths matching PATTERN, in addition to the configured ones (repeatable)
    #[arg(long, value_name = "PATTERN", action = clap::ArgAction::Append)]
    exclude: Vec<String>,
}

/// Wrap `text` in an ANSI colour, but only for an interactive terminal.
fn colorize(text: &str, color: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("TERM").as_deref() == Some(std::ffi::OsStr::new("dumb")) {
        return text.to_string();
    }
    if !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    format!("\x1b[{color}m{text}\x1b[0m")
}

fn print_diff(original: &str, formatted: &str, path: &Path) {
    let name = path.display().to_string();
    let diff = TextDiff::from_lines(original, formatted);
    let mut unified = diff.unified_diff();
    unified.header(&name, &name);
    print!("{unified}");
}

fn main() -> ExitCode {
    let args = Args::parse();
    ExitCode::from(run(args) as u8)
}

fn run(args: Args) -> i32 {
    if args.align_only && args.indent_only {
        eprintln!("error: --align-only and --indent-only are mutually exclusive");
        return 2;
    }
    if args.stdout && (args.check || args.diff) {
        eprintln!("error: --stdout cannot be combined with --check or --diff");
        return 2;
    }
    let options = FormatOptions {
        do_indent: !args.align_only,
        do_align: !args.indent_only,
        unit: args.indent,
    };

    if args.paths.len() == 1 && args.paths[0] == Path::new("-") {
        let mut input = String::new();
        if let Err(error) = std::io::stdin().read_to_string(&mut input) {
            eprintln!("error: {error}");
            return 2;
        }
        return match format_text(&input, &options) {
            Ok(formatted) => {
                print!("{formatted}");
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        };
    }

    let config = match load_config(args.config.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let config = if args.exclude.is_empty() {
        config
    } else {
        let mut exclude = config.exclude;
        exclude.extend(args.exclude.iter().cloned());
        Config {
            root: config.root,
            exclude,
        }
    };

    let search_paths = if args.paths.is_empty() {
        vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))]
    } else {
        args.paths.clone()
    };
    let paths = iter_sources(&search_paths, &config);
    if args.stdout && paths.len() != 1 {
        eprintln!("error: --stdout takes exactly one file to format, got {}", paths.len());
        return 2;
    }

    let (mark, verb) = if args.check {
        ("✗", "would reformat")
    } else {
        ("✔", "reformatted")
    };
    let mut changed = 0usize;
    let mut failed = false;
    for path in &paths {
        let original = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("error: {}: {error}", path.display());
                failed = true;
                continue;
            }
        };
        let formatted = match format_text(&original, &options) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("error: {}: refusing to format ({error})", path.display());
                failed = true;
                continue;
            }
        };
        if args.stdout {
            print!("{formatted}");
            continue;
        }
        if formatted == original {
            continue;
        }
        changed += 1;
        if args.diff {
            print_diff(&original, &formatted, path);
            continue;
        }
        if !args.check
            && let Err(error) = std::fs::write(path, &formatted)
        {
            eprintln!("error: {}: {error}", path.display());
            failed = true;
            continue;
        }
        if !args.quiet {
            // Reported as each file is written, so a long run shows progress.
            let color = if args.check { YELLOW } else { GREEN };
            println!("{} {verb} {}", colorize(mark, color), path.display());
            let _ = std::io::stdout().flush();
        }
    }

    if failed {
        return 2;
    }
    if changed > 0 && (args.check || args.diff) { 1 } else { 0 }
}

/// Load the configuration to use: an explicit `--config FILE`, or the first
/// one found searching upward from the working directory.
fn load_config(explicit: Option<&Path>) -> Result<Config, svfmt::SvFormatError> {
    match explicit {
        Some(path) => Ok(read_config(path)?.unwrap_or_else(|| Config {
            root: path
                .parent()
                .unwrap_or(Path::new("."))
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(".")),
            exclude: Vec::new(),
        })),
        None => find_config(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    }
}
