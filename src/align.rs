//! Group consecutive similar lines into runs and pad them into columns, so
//! declarations, port lists, struct members, assignments, named port
//! connections and trailing comments line up.

use crate::lexer::{Kind, Line, Token, is_assign_op, is_non_decl_keyword};
use crate::text::{ljust, rjust, width};

// ---------------------------------------------------------------------------------
// Cells
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Decl,
    Assign,
    Port,
    Label,
}

impl CellKind {
    /// One alignment character per column ('l' left, 'r' right).
    fn alignment(self) -> &'static [u8] {
        match self {
            CellKind::Decl => b"llllll",
            CellKind::Assign => b"lrll",
            CellKind::Port => b"lll",
            CellKind::Label => b"lll",
        }
    }
}

/// A classified line, decomposed into the columns it will be padded into.
#[derive(Clone, Debug)]
struct Cells {
    kind: CellKind,
    cells: Vec<String>,
}

impl Cells {
    fn with_terminator(&mut self, terminator: &str) {
        for index in (0..self.cells.len()).rev() {
            if !self.cells[index].is_empty() {
                self.cells[index].push_str(terminator);
                return;
            }
        }
        let last = self.cells.len() - 1;
        self.cells[last] = terminator.to_string();
    }
}

fn words(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn dims(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.as_str()).collect::<String>()
}

/// Split one declaration head into (type+packed, name+unpacked, name start).
fn split_decl_head(tokens: &[Token]) -> Option<(String, String, usize)> {
    let mut end = tokens.len();
    while end > 0 && tokens[end - 1].kind == Kind::Brackets {
        end -= 1;
    }
    if end < 2 || tokens[end - 1].kind != Kind::Word {
        return None;
    }
    let unpacked = &tokens[end..];
    let name = &tokens[end - 1];
    let body = &tokens[..end - 1];

    let mut split = body.len();
    while split > 0 && body[split - 1].kind == Kind::Brackets {
        split -= 1;
    }
    let (types, packed) = (&body[..split], &body[split..]);
    if types.is_empty() || types.iter().any(|t| t.kind != Kind::Word) {
        return None;
    }

    let mut declared_type = words(types);
    if !packed.is_empty() {
        declared_type = format!("{declared_type} {}", dims(packed));
    }
    let declared = if !unpacked.is_empty() {
        format!("{} {}", name.text, dims(unpacked))
    } else {
        name.text.clone()
    };
    Some((declared_type, declared, name.start))
}

/// Split off the trailing `,`/`;` and any delimiters closed on the way.
///
/// Returns the remaining tokens and the tail text (`,`, `);` ...), or `None`
/// when the line does not end a statement or list entry.
fn peel_tail(tokens: &[Token], code: &str, listed: bool) -> Option<(Vec<Token>, String)> {
    let mut end = tokens.len();
    let mut terminated = false;
    if end > 0 && tokens[end - 1].kind == Kind::Punct && matches!(tokens[end - 1].text.as_str(), "," | ";") {
        end -= 1;
        terminated = true;
    }
    let mut start = end;
    while start > 0 && tokens[start - 1].kind == Kind::Punct && matches!(tokens[start - 1].text.as_str(), ")" | "]" | "}") {
        start -= 1;
    }
    if start < end && !listed {
        return None;
    }
    if !terminated && start == end && !listed {
        return None;
    }
    if start == 0 {
        return None;
    }
    let tail = if start < tokens.len() {
        code[tokens[start].start..].trim().to_string()
    } else {
        String::new()
    };
    Some((tokens[..start].to_vec(), tail))
}

/// Split a declaration into (attr, type+packed, name+unpacked, =, value).
fn split_decl(tokens: &[Token], code: &str) -> Option<Cells> {
    if tokens.len() < 2 {
        return None;
    }
    let mut body: Vec<Token> = tokens.to_vec();
    let stop = body[body.len() - 1].end;

    let mut index = 0;
    let mut attrs: Vec<Token> = Vec::new();
    while index < body.len() && body[index].kind == Kind::Attr {
        attrs.push(body[index].clone());
        index += 1;
    }
    body = body[index..].to_vec();
    if body.is_empty() || body[0].kind != Kind::Word || is_non_decl_keyword(&body[0].text) {
        return None;
    }

    let mut value = String::new();
    let mut equals = String::new();
    for position in 0..body.len() {
        let token = body[position].clone();
        if token.kind == Kind::Op && token.text == "=" {
            if position == 0 {
                return None;
            }
            equals = "=".to_string();
            value = code[token.end..stop].trim().to_string();
            body.truncate(position);
            break;
        }
    }

    // A declaration of several names shares the type column with its
    // neighbours; the name list itself is left as written.
    if body.iter().any(|t| t.kind == Kind::Punct && t.text == ",") {
        if !equals.is_empty() || body.iter().any(|t| !matches!(t.kind, Kind::Word | Kind::Brackets | Kind::Punct)) {
            return None;
        }
        if body.iter().any(|t| t.kind == Kind::Punct && t.text != ",") {
            return None;
        }
        let first_comma = body.iter().position(|t| t.kind == Kind::Punct && t.text == ",").unwrap();
        let (declared_type, _, declared_start) = split_decl_head(&body[..first_comma])?;
        return Some(Cells {
            kind: CellKind::Decl,
            cells: vec![
                words(&attrs),
                declared_type,
                code[declared_start..stop].trim().to_string(),
                String::new(),
                String::new(),
                String::new(),
            ],
        });
    }

    if body.iter().any(|t| !matches!(t.kind, Kind::Word | Kind::Brackets)) {
        return None;
    }

    let (declared_type, declared, _) = split_decl_head(&body)?;
    Some(Cells {
        kind: CellKind::Decl,
        cells: vec![words(&attrs), declared_type, declared, equals, value, String::new()],
    })
}

/// True for a bare name with selects and fields: `a`, `a.b[3]`, ...
///
/// Works off the token stream rather than the text so that nested brackets --
/// `arr[IDX[i]].field` -- stay in one BRACKETS token instead of tripping up a
/// bracket-matching regex. `concatenation` also admits a leading `{...}`.
fn is_name(tokens: &[Token], concatenation: bool) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let head_ok = if concatenation {
        matches!(tokens[0].kind, Kind::Word | Kind::Braces)
    } else {
        tokens[0].kind == Kind::Word
    };
    if !head_ok {
        return false;
    }
    if tokens[0].kind == Kind::Word && is_non_decl_keyword(&tokens[0].text) {
        return false;
    }
    let mut index = 1;
    while index < tokens.len() {
        if tokens[index].kind == Kind::Brackets {
            index += 1;
        } else if tokens[index].kind == Kind::Punct && tokens[index].text == "." {
            if index + 1 >= tokens.len() || tokens[index + 1].kind != Kind::Word {
                return false;
            }
            index += 2;
        } else {
            return false;
        }
    }
    true
}

/// True for a plain assignment target, with or without a leading `assign`.
fn is_lvalue(tokens: &[Token]) -> bool {
    let tokens = if !tokens.is_empty() && tokens[0].kind == Kind::Word && tokens[0].text == "assign" {
        &tokens[1..]
    } else {
        tokens
    };
    is_name(tokens, true)
}

/// Split an assignment statement into (lhs, operator, rhs).
fn split_assign(tokens: &[Token], code: &str) -> Option<Cells> {
    if tokens.is_empty() {
        return None;
    }
    let stop = tokens[tokens.len() - 1].end;
    for position in 0..tokens.len() {
        let token = &tokens[position];
        if token.kind != Kind::Op || !is_assign_op(&token.text) {
            continue;
        }
        if position == 0 {
            return None;
        }
        let lhs = code[..token.start].trim().to_string();
        if !is_lvalue(&tokens[..position]) {
            return None;
        }
        let rhs = code[token.end..stop].trim().to_string();
        if rhs.is_empty() {
            return None;
        }
        return Some(Cells {
            kind: CellKind::Assign,
            cells: vec![lhs, token.text.clone(), rhs, String::new()],
        });
    }
    None
}

/// Split a named port/parameter connection into (.name, (expr)).
fn split_port(tokens: &[Token]) -> Option<Cells> {
    if tokens.len() != 3 {
        return None;
    }
    let (dot, name, expr) = (&tokens[0], &tokens[1], &tokens[2]);
    if dot.kind != Kind::Punct || dot.text != "." || name.kind != Kind::Word || expr.kind != Kind::Parens {
        return None;
    }
    Some(Cells {
        kind: CellKind::Port,
        cells: vec![format!(".{}", name.text), expr.text.clone(), String::new()],
    })
}

/// Split `label: rest` (structure literal fields, case items).
fn split_label(tokens: &[Token], code: &str) -> Option<Cells> {
    if tokens.len() < 2 {
        return None;
    }
    let stop = tokens[tokens.len() - 1].end;
    let colons: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == Kind::Punct && t.text == ":")
        .map(|(i, _)| i)
        .collect();
    if colons.len() != 1 || colons[0] == 0 {
        return None;
    }
    let colon = &tokens[colons[0]];
    let label = code[..colon.start].trim().to_string();
    let rest = code[colon.end..stop].trim().to_string();
    if rest.is_empty() {
        return None;
    }
    let head = &tokens[..colons[0]];
    if head.iter().any(|t| t.kind == Kind::Punct && t.text == ",") {
        return None;
    }
    // Only a name-shaped head is a label. Anything else -- an operator, a
    // call, a cast -- means the `:` belongs to a ternary whose `?` sits on an
    // earlier line, and gluing it to what precedes it would misread the
    // expression.
    if !is_name(head, false) && label != "default" {
        return None;
    }
    Some(Cells {
        kind: CellKind::Label,
        cells: vec![format!("{label}:"), rest, String::new()],
    })
}

/// Decompose `line` into aligned cells, or `None` when it is left alone.
fn classify(line: &Line) -> Option<Cells> {
    if line.protected || line.in_block_comment || line.directive || line.tokens.is_empty() {
        return None;
    }
    let (body, terminator) = peel_tail(&line.tokens, &line.code, line.depth > 0)?;
    let code = &line.code;
    let mut cells = split_decl(&body, code)
        .or_else(|| split_assign(&body, code))
        .or_else(|| split_port(&body))
        .or_else(|| split_label(&body, code))?;
    cells.with_terminator(&terminator);
    let last = cells.cells.len() - 1;
    cells.cells[last] = line.comment.clone();
    Some(cells)
}

/// Pad one run of like lines into columns, rewriting `output` in place.
///
/// A run of one still comes through here: with nothing to line up against,
/// every column is its own width, so the line collapses to single spaces
/// between its cells.
///
/// A column is widened only by the lines that have something after it. A
/// line whose last cell falls in that column needs no padding there --
/// nothing follows it -- so it must not push the column out for everyone
/// else. That is what keeps `logic injector_reset;` from moving the `=` of a
/// neighbouring `localparam int RAM_DEPTH = 8192;`, and one long value from
/// shoving every trailing comment to the right.
fn render_run(entries: &[(usize, Cells)], output: &mut [String]) {
    let columns = entries[0].1.cells.len();
    let mut widths = vec![0usize; columns];
    for (_, cells) in entries {
        let last = cells
            .cells
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty())
            .map(|(i, _)| i)
            .max()
            .unwrap_or(0);
        for (index, w) in widths.iter_mut().enumerate().take(last) {
            *w = (*w).max(width(&cells.cells[index]));
        }
    }

    let alignment = entries[0].1.kind.alignment();
    for (key, cells) in entries {
        let mut parts: Vec<String> = Vec::new();
        for (index, text) in cells.cells[..cells.cells.len() - 1].iter().enumerate() {
            if widths[index] > 0 || !text.is_empty() {
                parts.push(if alignment[index] == b'r' {
                    rjust(text, widths[index])
                } else {
                    ljust(text, widths[index])
                });
            }
        }
        let code = parts.join(" ");
        let comment = &cells.cells[cells.cells.len() - 1];
        let leading = leading_whitespace(&output[*key]);
        let body = if !comment.is_empty() { format!("{code} {comment}") } else { code };
        output[*key] = format!("{leading}{body}").trim_end().to_string();
    }
}

/// Align runs of like lines, returning the rewritten text of each.
pub fn align(lines: &[Line], rendered: &[String]) -> Vec<String> {
    let mut output = rendered.to_vec();
    let mut run: Vec<(usize, Cells)> = Vec::new();
    let mut signature: Option<(CellKind, String, bool)> = None;

    macro_rules! flush {
        () => {
            if !run.is_empty() {
                render_run(&run, &mut output);
                run.clear();
            }
        };
    }

    for (index, line) in lines.iter().enumerate() {
        if line.protected || line.in_block_comment {
            flush!();
            signature = None;
            continue;
        }
        if line.blank() {
            // A blank line only separates blocks at statement level; inside a
            // port list or struct body the whole list aligns as one unit.
            if line.depth == 0 {
                flush!();
                signature = None;
            }
            continue;
        }
        if line.comment_only() || attribute_only(line) {
            // Comments and standalone (* attribute *) lines annotate the
            // declaration below them; they do not end a run.
            continue;
        }
        let cells = match classify(line) {
            Some(c) => c,
            None => {
                flush!();
                signature = None;
                continue;
            }
        };
        // The attribute cell is the first column, so a run must not mix lines
        // that have one with lines that do not: padding an empty leading
        // cell would read as indentation and fight the indent pass.
        let here = (cells.kind, leading_whitespace(&output[index]), !cells.cells[0].is_empty());
        if signature.as_ref() != Some(&here) {
            flush!();
            signature = Some(here);
        }
        run.push((index, cells));
    }
    flush!();
    output
}

/// True for a line holding nothing but an `(* ... *)` attribute.
fn attribute_only(line: &Line) -> bool {
    line.tokens.len() == 1 && line.tokens[0].kind == Kind::Attr
}

fn leading_whitespace(text: &str) -> String {
    let lead = text.len() - text.trim_start().len();
    text[..lead].to_string()
}
