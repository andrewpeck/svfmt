//! Driver that runs the header, indent and align passes and verifies the
//! result is token-identical to the input.

use crate::align::align;
use crate::header::reflow_headers;
use crate::indent::indent;
use crate::lexer::{scan_lines, token_stream};
use std::fmt;

/// Raised when formatting would have changed something other than whitespace.
#[derive(Debug, Clone)]
pub struct SvFormatError(pub String);

impl fmt::Display for SvFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SvFormatError {}

#[derive(Clone, Copy, Debug)]
pub struct FormatOptions {
    pub do_indent: bool,
    pub do_align: bool,
    pub unit: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            do_indent: true,
            do_align: true,
            unit: 2,
        }
    }
}

/// Format `text`, changing whitespace only.
///
/// Returns [`SvFormatError`] if the result is not token-identical to the
/// input, so a formatter bug can never silently corrupt a source file.
pub fn format_text(text: &str, options: &FormatOptions) -> Result<String, SvFormatError> {
    let mut lines = scan_lines(&reflow_headers(text, options.unit));
    let mut rendered: Vec<String> = if options.do_indent {
        indent(&lines, options.unit)
    } else {
        lines.iter().map(|l| l.raw.trim_end().to_string()).collect()
    };
    if options.do_align {
        if options.do_indent {
            lines = scan_lines(&rendered.join("\n"));
        }
        rendered = align(&lines, &rendered);
    }
    let result = rendered.join("\n");

    let before = token_stream(text);
    let after = token_stream(&result);
    if before != after {
        return Err(SvFormatError(first_difference(&before, &after)));
    }
    Ok(result)
}

/// Format `text` with both passes enabled at the default indent width.
pub fn format_default(text: &str) -> Result<String, SvFormatError> {
    format_text(text, &FormatOptions::default())
}

fn first_difference(before: &[String], after: &[String]) -> String {
    for (index, (left, right)) in before.iter().zip(after.iter()).enumerate() {
        if left != right {
            return format!("token {index} changed: {left:?} -> {right:?}");
        }
    }
    format!("token count changed: {} -> {}", before.len(), after.len())
}
