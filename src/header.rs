//! Park a module header's `#(` and `(` at the end of the line before them.
//!
//! A parameter or port list keeps its opening delimiter on the line that
//! precedes its entries, so every module header reads the same way:
//!
//! ```verilog
//! module adder_tree_top #(
//!   int N = 8
//! ) (
//!   input logic clk
//! );
//! ```
//!
//! Only line breaks inside the header move: an opener that dangles at the
//! start of its own line is pulled up onto the text before it, and a list
//! that starts on the same line as its opener is pushed down. A list that
//! also closes on that line is short enough to leave as written.

use crate::lexer::{Line, find_from, scan_lines, scan_string};
use crate::text::{char_at, starts_with_at};

/// declarations whose header carries a parameter list and a port list
fn is_header_word(word: &str) -> bool {
    matches!(word, "module" | "macromodule" | "interface" | "program")
}

/// The word at the very start of `code`, if it is a valid identifier start.
fn first_word(code: &str) -> Option<&str> {
    let mut chars = code.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for c in chars {
        if c.is_ascii_alphanumeric() || c == '_' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    Some(&code[..end])
}

/// True for a line that begins with the opener of a parameter or port list:
/// an optional `#` (and whitespace) followed by `(` not immediately followed
/// by `*`.
fn is_list_opener(code: &str) -> bool {
    let rest = match code.strip_prefix('#') {
        Some(r) => r.trim_start(),
        None => code,
    };
    rest.starts_with('(') && !rest.starts_with("(*")
}

/// Walk `code` from nesting `depth`.
///
/// Returns the index of the first delimiter left open at the end of the
/// line, the depth that delimiter opened from, whether a `;` closed the
/// header, and the depth the line ends at. Strings, comments and `(* ... *)`
/// attributes are stepped over rather than counted.
fn header_scan(code: &str, depth_in: i32) -> (Option<usize>, i32, bool, i32) {
    let mut opener: Option<usize> = None;
    let mut opener_depth = 0i32;
    let mut ended = false;
    let mut pending = 0i32;
    let mut depth = depth_in;
    let mut i = 0usize;
    let n = code.len();
    while i < n {
        let ch = match char_at(code, i) {
            Some(c) => c,
            None => break,
        };
        if ch == '"' {
            i = scan_string(code, i);
            continue;
        }
        if starts_with_at(code, i, "(*") && !starts_with_at(code, i, "(*)") {
            i = match find_from(code, i + 2, "*)") {
                Some(p) => p + 2,
                None => n,
            };
            continue;
        }
        if starts_with_at(code, i, "//") {
            break;
        }
        if starts_with_at(code, i, "/*") {
            i = match find_from(code, i + 2, "*/") {
                Some(p) => p + 2,
                None => n,
            };
            continue;
        }
        match ch {
            '(' | '[' | '{' => {
                if pending == 0 {
                    opener = Some(i);
                    opener_depth = depth;
                }
                pending += 1;
                depth += 1;
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if pending > 0 {
                    pending -= 1;
                    if pending == 0 {
                        opener = None;
                    }
                }
            }
            ';' if depth == 0 => ended = true,
            _ => {}
        }
        i += ch.len_utf8();
    }
    (opener, opener_depth, ended, depth)
}

/// True for the `module`/`interface`/... word that opens a header.
fn is_header_head(line: &Line) -> bool {
    if line.protected || line.in_block_comment || line.directive || line.depth != 0 {
        return false;
    }
    matches!(first_word(&line.code), Some(w) if is_header_word(w))
}

struct HeadEntry {
    indent: String,
    code: String,
    comment: String,
    gap: String,
}

struct Reflow {
    out: Vec<String>,
    head: Option<HeadEntry>,
    in_header: bool,
    depth: i32,
    unit: usize,
}

impl Reflow {
    fn emit(&mut self, indent: &str, code: &str, comment: &str, gap: &str) {
        let mut rendered = format!("{indent}{code}");
        if !comment.is_empty() {
            rendered = if !code.is_empty() {
                format!("{rendered}{gap}{comment}")
            } else {
                format!("{indent}{comment}")
            };
        }
        self.out.push(rendered.trim_end().to_string());
    }

    /// Emit the pending header line, pushing down a list that starts on it.
    fn settle(&mut self, code: &str) {
        let head = self.head.take().expect("settle called with no pending header line");
        let (opener, opener_depth, _, _) = header_scan(code, self.depth);
        let tail = match opener {
            Some(op) if opener_depth == 0 => code[op + 1..].trim().to_string(),
            _ => String::new(),
        };
        if tail.is_empty() {
            self.emit(&head.indent, code, &head.comment, &head.gap);
            return;
        }
        let cut = opener.unwrap() + 1; // the opening delimiter is one ASCII byte
        self.emit(&head.indent, &code[..cut], "", &head.gap);
        let indent = format!("{}{}", head.indent, " ".repeat(self.unit));
        self.emit(&indent, &tail, &head.comment, &head.gap);
    }
}

pub fn reflow_headers(text: &str, unit: usize) -> String {
    let lines = scan_lines(text);
    let mut state = Reflow {
        out: Vec::new(),
        head: None,
        in_header: false,
        depth: 0,
        unit,
    };

    for line in &lines {
        let skip = line.protected || line.in_block_comment || line.directive;
        let entry_code: String;
        let merges =
            state.head.as_ref().is_some_and(|h| h.comment.is_empty()) && !skip && !line.code.is_empty() && is_list_opener(&line.code);
        if merges {
            let head = state.head.as_ref().unwrap();
            let new_code = format!("{} {}", head.code, line.code);
            let new_head = HeadEntry {
                indent: head.indent.clone(),
                code: new_code.clone(),
                comment: line.comment.clone(),
                gap: line.gap.clone(),
            };
            state.head = Some(new_head);
            entry_code = new_code;
        } else {
            if state.head.is_some() {
                let pending = state.head.as_ref().unwrap().code.clone();
                state.settle(&pending);
                state.depth = header_scan(&pending, state.depth).3;
            }
            if skip {
                state.in_header = false;
            }
            if skip || line.code.is_empty() {
                state.out.push(if line.raw.trim().is_empty() {
                    String::new()
                } else {
                    line.raw.trim_end().to_string()
                });
                continue;
            }
            state.in_header = state.in_header || is_header_head(line);
            if !state.in_header {
                state.out.push(line.raw.trim_end().to_string());
                state.depth = 0;
                continue;
            }
            let new_head = HeadEntry {
                indent: line.indent.clone(),
                code: line.code.clone(),
                comment: line.comment.clone(),
                gap: line.gap.clone(),
            };
            entry_code = new_head.code.clone();
            state.head = Some(new_head);
        }

        let (_, _, ended, after) = header_scan(&entry_code, state.depth);
        if after != 0 || ended {
            state.settle(&entry_code);
            state.depth = after;
            state.in_header = state.in_header && !ended;
        }
    }

    if state.head.is_some() {
        let pending = state.head.as_ref().unwrap().code.clone();
        state.settle(&pending);
    }
    state.out.join("\n")
}
