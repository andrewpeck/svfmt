//! Re-indent lines from a stack of open blocks and delimiters.

use crate::lexer::{
    Kind, Line, Token, block_closer_opens, closer_for, find_from, is_assign_op, is_block_opener, is_body_head, is_body_head_extra,
    is_leading_only_opener, is_opener_modifier, is_word_body, scan_string, split_comment,
};
use crate::text::{char_at, starts_with_at, width};

/// An open ()/[]/{} delimiter spanning more than one line.
struct Delim {
    ch: char,
    /// column of the delimiter character itself
    char_col: i32,
    /// column the delimiter's token starts at (`'{` starts one column earlier)
    token_col: i32,
    /// indent of the line the delimiter was opened on
    base: i32,
    /// indent of the statement the delimiter belongs to
    stmt_base: i32,
    /// True when something other than whitespace follows the delimiter
    content_after: bool,
}

struct Block {
    keyword: String,
    base: i32,
}

enum DelimEvent {
    Open {
        ch: char,
        char_col: i32,
        token_col: i32,
        content_after: bool,
    },
    Close,
}

/// Return (event, char, char_col, token_col, content_after) for each delimiter.
fn delim_events(code: &str) -> Vec<DelimEvent> {
    let mut events = Vec::new();
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
        if matches!(ch, '(' | '[' | '{') {
            let token_byte = if ch == '{' && i > 0 && char_at(code, i - 1) == Some('\'') {
                i - 1
            } else {
                i
            };
            let (rest, _, _) = split_comment(&code[i + ch.len_utf8()..], false);
            let char_col = width(&code[..i]) as i32;
            let token_col = width(&code[..token_byte]) as i32;
            events.push(DelimEvent::Open {
                ch,
                char_col,
                token_col,
                content_after: !rest.trim().is_empty(),
            });
        } else if matches!(ch, ')' | ']' | '}') {
            events.push(DelimEvent::Close);
        }
        i += ch.len_utf8();
    }
    events
}

/// Words of `code` that sit outside any delimiter, string, or comment.
///
/// Unlike [`crate::lexer::tokenize`] this never fails, so block keywords are
/// still found on lines that leave a delimiter open (`module foo # (`).
fn scan_words(code: &str, start_depth: i32) -> Vec<String> {
    let mut words = Vec::new();
    let mut depth = start_depth;
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
        if matches!(ch, '(' | '[' | '{') {
            depth += 1;
        } else if matches!(ch, ')' | ']' | '}') {
            depth = (depth - 1).max(0);
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < n {
                match char_at(code, i) {
                    Some(c) if is_word_body(c) => i += c.len_utf8(),
                    _ => break,
                }
            }
            if depth == 0 {
                words.push(code[start..i].to_string());
            }
            continue;
        }
        i += ch.len_utf8();
    }
    words
}

fn is_opener(words: &[String], position: usize) -> bool {
    let word = &words[position];
    if !is_block_opener(word) {
        return false;
    }
    if !is_leading_only_opener(word) {
        return true;
    }
    (0..position).all(|earlier| is_opener_modifier(&words[earlier]))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatementState {
    Done,
    Continued,
    Body,
}

/// Classify how a line leaves the statement in progress.
fn statement_state(tokens: &[Token]) -> StatementState {
    let last = &tokens[tokens.len() - 1];
    if last.kind == Kind::Punct && last.text == ";" {
        return StatementState::Done;
    }
    // A line that opens or closes a block finishes whatever came before it;
    // the block's own contents are indented from the block stack instead.
    if tokens
        .iter()
        .any(|t| t.kind == Kind::Word && (is_block_opener(&t.text) || block_closer_opens(&t.text).is_some()))
    {
        return StatementState::Done;
    }
    if last.kind == Kind::Punct && last.text == ":" {
        // ... unless it is the tail of a ternary or an assignment continuation.
        let ternary = tokens
            .iter()
            .any(|t| t.kind == Kind::Op && (t.text == "?" || is_assign_op(&t.text)));
        return if ternary { StatementState::Continued } else { StatementState::Body };
    }
    let first = tokens[0].text.as_str();
    if is_body_head(first) || is_body_head_extra(first) || last.text == "else" {
        return StatementState::Body;
    }
    StatementState::Continued
}

/// Re-indent `lines`, returning the rendered text of each.
pub fn indent(lines: &[Line], unit: usize) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    let mut delims: Vec<Delim> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending: Vec<i32> = Vec::new();
    let mut closed: Vec<i32> = Vec::new();
    let mut continued = false;
    let mut macro_continued = false;
    let mut statement_indent: i32 = 0;
    let mut statement_assign = false;
    let unit_i = unit as i32;

    for line in lines {
        let raw_indent = width(&line.indent) as i32;
        let skip =
            line.protected || line.in_block_comment || line.directive || macro_continued || (line.blank() && line.raw.trim().is_empty());
        if skip {
            output.push(if line.raw.trim().is_empty() {
                String::new()
            } else {
                line.raw.trim_end().to_string()
            });
            if line.directive || macro_continued {
                macro_continued = line.raw.trim_end().ends_with('\\');
            }
            continue;
        }

        let first_word = line.tokens.first().map(|t| t.text.as_str()).unwrap_or("");
        let target = if first_word == "else" && !closed.is_empty() && delims.is_empty() {
            // Re-open the body the matching if/assert closed, so `else` lines up.
            closed.pop().unwrap()
        } else {
            if !first_word.is_empty() && delims.is_empty() {
                closed.clear();
            }
            target_indent(
                line,
                &delims,
                &blocks,
                &pending,
                continued,
                raw_indent,
                unit_i,
                statement_indent,
                statement_assign,
            )
        };
        let target = if line.comment_only() && raw_indent > target && delims.is_empty() {
            // Deliberately hanging documentation comments keep their column.
            // Inside a list they do not: the entries they annotate are
            // pinned to the list's own column, and a comment left behind at
            // a column of its own reads as belonging to something else.
            raw_indent
        } else {
            target
        };
        output.push(line.render(&" ".repeat(target.max(0) as usize), &line.code, &line.comment));

        if line.comment_only() {
            continue;
        }

        if delims.is_empty() && !continued {
            statement_indent = target;
            statement_assign = false;
        }
        statement_assign = statement_assign || line.tokens.iter().any(|t| t.kind == Kind::Op && is_assign_op(&t.text));
        advance(line, &mut delims, &mut blocks, target, statement_indent);

        let state = if !line.tokens.is_empty() && delims.is_empty() {
            statement_state(&line.tokens)
        } else {
            StatementState::Done
        };
        continued = state == StatementState::Continued;
        if !line.tokens.is_empty() && delims.is_empty() {
            match state {
                StatementState::Done => {
                    // One statement closes every single-statement body it
                    // opened; remember them in case the next line is an `else`.
                    closed = pending.clone();
                    pending.clear();
                }
                StatementState::Body => pending.push(target),
                StatementState::Continued => {}
            }
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn target_indent(
    line: &Line,
    delims: &[Delim],
    blocks: &[Block],
    pending: &[i32],
    continued: bool,
    raw_indent: i32,
    unit: i32,
    statement_indent: i32,
    statement_assign: bool,
) -> i32 {
    if let Some(top) = delims.last() {
        let opener_columns = [top.token_col, top.char_col, top.char_col + 1];
        let first = line.code.chars().next();
        if first.is_some() && first == closer_for(top.ch) {
            // A line that starts by closing the delimiter lines up with the
            // line that opened it, or -- when the list hangs off the
            // opening token -- with that token. A closer sitting exactly
            // one unit in is not evidence of hanging, though: that is where
            // the entries of a list opened at the end of its line go, and
            // the closer belongs at the column the list was opened from.
            if raw_indent == top.base || (opener_columns.contains(&raw_indent) && raw_indent != top.base + unit) {
                return raw_indent;
            }
            if top.content_after && raw_indent >= top.stmt_base + unit {
                return raw_indent;
            }
            return top.base;
        }
        if top.content_after {
            // The author is aligning under something on the opening line;
            // any indent at least as deep as a hanging indent is left alone.
            return if raw_indent >= top.stmt_base + unit {
                raw_indent
            } else {
                top.char_col + 1
            };
        }
        return if opener_columns.contains(&raw_indent) || raw_indent == top.base + unit {
            raw_indent
        } else {
            top.base + unit
        };
    }

    if continued {
        // An expression broken across lines keeps whatever column the
        // author aligned it to; a statement with no operator in it -- the
        // module name and instance name of an instantiation -- lines up
        // with its own head.
        return if statement_assign { raw_indent } else { statement_indent };
    }

    let base = blocks.last().map_or(0, |b| b.base + unit);
    if let Some(first_tok) = line.tokens.first() {
        if let Some(keyword) = block_closer_opens(&first_tok.text) {
            for block in blocks.iter().rev() {
                if block.keyword == keyword {
                    return block.base;
                }
            }
            return (base - unit).max(0);
        }
        if first_tok.text == "begin"
            && let Some(&p) = pending.last()
        {
            return p;
        }
    }

    if let Some(&p) = pending.last() {
        return p + unit;
    }
    base
}

/// Update the block/delimiter stacks for a line placed at `target`.
fn advance(line: &Line, delims: &mut Vec<Delim>, blocks: &mut Vec<Block>, target: i32, statement_indent: i32) {
    let words = scan_words(&line.code, delims.len() as i32);
    for (position, word) in words.iter().enumerate() {
        if let Some(keyword) = block_closer_opens(word) {
            if let Some(index) = blocks.iter().rposition(|b| b.keyword == keyword) {
                blocks.truncate(index);
            }
        } else if is_opener(&words, position) {
            let keyword = if matches!(word.as_str(), "casex" | "casez" | "randcase") {
                "case".to_string()
            } else {
                word.clone()
            };
            blocks.push(Block { keyword, base: target });
        }
    }

    for event in delim_events(&line.code) {
        match event {
            DelimEvent::Open {
                ch,
                char_col,
                token_col,
                content_after,
            } => {
                let stmt_base = delims.last().map_or(statement_indent, |d| d.stmt_base);
                delims.push(Delim {
                    ch,
                    char_col: target + char_col,
                    token_col: target + token_col,
                    base: target,
                    stmt_base,
                    content_after,
                });
            }
            DelimEvent::Close => {
                delims.pop();
            }
        }
    }
}
