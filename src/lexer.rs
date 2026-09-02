//! Tokenizing and line-splitting for (System)Verilog source.
//!
//! Mirrors the Python lexer one-for-one: `tokenize` turns a line of code into
//! balanced-group tokens (or `None` when a delimiter is left open), and
//! `scan_lines` splits a whole file into annotated [`Line`] records.

use crate::text::{char_at, starts_with_at};
use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------------
// Token kinds
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    Brackets,
    Parens,
    Braces,
    Attr,
    Op,
    Punct,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

impl Token {
    fn new(kind: Kind, text: &str, start: usize, end: usize) -> Self {
        Token {
            kind,
            text: text.to_string(),
            start,
            end,
        }
    }
}

/// Longest first so that the scanner never splits a compound operator.
pub const OPERATORS: &[&str] = &[
    "<<<=", ">>>=", "<<=", ">>=", "===", "!==", "==?", "!=?", "<->", "->>", "**", "==", "!=", "<=", ">=", "&&", "||", "->", "<<", ">>",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "++", "--", "::", "~&", "~|", "~^", "^~", "=", "+", "-", "*", "/", "%", "<", ">", "!",
    "~", "&", "|", "^", "?", "@", "#",
];

pub fn closer_for(opener: char) -> Option<char> {
    match opener {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn is_word_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '\\' | '`' | '\'')
}

pub(crate) fn is_word_body(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '\'')
}

/// The character class `[A-Za-z_$`0-9']` used to flatten a line into the
/// whitespace-insensitive token stream checked before/after formatting.
fn is_flat_atom_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '`' | '\'')
}

pub fn is_assign_op(op: &str) -> bool {
    matches!(
        op,
        "=" | "<=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>="
    )
}

// ---------------------------------------------------------------------------------
// Grammar keyword sets
// ---------------------------------------------------------------------------------

/// Keywords that can never begin a declaration.
pub fn is_non_decl_keyword(word: &str) -> bool {
    matches!(
        word,
        "always"
            | "always_comb"
            | "always_ff"
            | "always_latch"
            | "and"
            | "assert"
            | "assign"
            | "assume"
            | "begin"
            | "break"
            | "case"
            | "casex"
            | "casez"
            | "continue"
            | "cover"
            | "default"
            | "disable"
            | "do"
            | "else"
            | "end"
            | "endcase"
            | "endclass"
            | "endfunction"
            | "endgenerate"
            | "endinterface"
            | "endmodule"
            | "endpackage"
            | "endprogram"
            | "endproperty"
            | "endsequence"
            | "endtask"
            | "final"
            | "for"
            | "foreach"
            | "forever"
            | "fork"
            | "generate"
            | "if"
            | "initial"
            | "import"
            | "export"
            | "include"
            | "join"
            | "join_any"
            | "join_none"
            | "module"
            | "package"
            | "program"
            | "property"
            | "repeat"
            | "return"
            | "sequence"
            | "task"
            | "unique"
            | "unique0"
            | "priority"
            | "while"
            | "wait"
    )
}

pub fn is_block_opener(word: &str) -> bool {
    matches!(
        word,
        "begin"
            | "fork"
            | "generate"
            | "case"
            | "casex"
            | "casez"
            | "randcase"
            | "module"
            | "interface"
            | "package"
            | "program"
            | "class"
            | "function"
            | "task"
            | "covergroup"
            | "property"
            | "sequence"
            | "clocking"
            | "checker"
            | "table"
            | "specify"
            | "primitive"
            | "config"
    )
}

/// The block keyword a closer word ends, e.g. `"end"` -> `"begin"`.
pub fn block_closer_opens(word: &str) -> Option<&'static str> {
    Some(match word {
        "end" => "begin",
        "join" | "join_any" | "join_none" => "fork",
        "endcase" => "case",
        "endgenerate" => "generate",
        "endmodule" => "module",
        "endinterface" => "interface",
        "endpackage" => "package",
        "endprogram" => "program",
        "endclass" => "class",
        "endfunction" => "function",
        "endtask" => "task",
        "endgroup" => "covergroup",
        "endproperty" => "property",
        "endsequence" => "sequence",
        "endclocking" => "clocking",
        "endchecker" => "checker",
        "endtable" => "table",
        "endspecify" => "specify",
        "endprimitive" => "primitive",
        "endconfig" => "config",
        _ => return None,
    })
}

/// Statement heads whose body is indented when it is not wrapped in begin/end.
pub fn is_body_head(word: &str) -> bool {
    matches!(
        word,
        "if" | "else"
            | "for"
            | "foreach"
            | "while"
            | "repeat"
            | "forever"
            | "do"
            | "always"
            | "always_comb"
            | "always_ff"
            | "always_latch"
            | "initial"
            | "final"
    )
}

pub fn is_body_head_extra(word: &str) -> bool {
    matches!(word, "assert" | "assume" | "cover" | "expect")
}

/// Openers that are only recognised at the start of a line.
pub fn is_leading_only_opener(word: &str) -> bool {
    matches!(
        word,
        "module"
            | "interface"
            | "package"
            | "program"
            | "class"
            | "function"
            | "task"
            | "covergroup"
            | "property"
            | "sequence"
            | "clocking"
            | "checker"
            | "table"
            | "specify"
            | "primitive"
            | "config"
    )
}

/// Words that may precede a leading-only opener.
pub fn is_opener_modifier(word: &str) -> bool {
    matches!(
        word,
        "virtual" | "static" | "automatic" | "protected" | "local" | "pure" | "extern" | "const" | "typedef" | "rand" | "randc"
    )
}

// ---------------------------------------------------------------------------------
// Low-level scanning
// ---------------------------------------------------------------------------------

pub(crate) fn find_from(s: &str, i: usize, pat: &str) -> Option<usize> {
    s.get(i..)?.find(pat).map(|p| p + i)
}

/// Return the index just past the string literal starting at `i`.
pub fn scan_string(text: &str, i: usize) -> usize {
    let mut i = i + 1; // opening quote is one (ASCII) byte
    let n = text.len();
    while i < n {
        let c = match char_at(text, i) {
            Some(c) => c,
            None => break,
        };
        if c == '\\' {
            let after = i + c.len_utf8();
            i = match char_at(text, after) {
                Some(c2) => after + c2.len_utf8(),
                None => after,
            };
            continue;
        }
        if c == '"' {
            return i + c.len_utf8();
        }
        i += c.len_utf8();
    }
    i
}

/// Split `text` into (code, trailing comment, still-in-block-comment).
///
/// Only a comment that runs to the end of the line is peeled off; a block
/// comment with code after it stays part of the code so nothing is reordered.
pub fn split_comment(text: &str, in_block_comment: bool) -> (String, String, bool) {
    let mut in_block_comment = in_block_comment;
    let mut i = 0usize;
    let n = text.len();
    let mut comment_start: Option<usize> = None;
    while i < n {
        if in_block_comment {
            match find_from(text, i, "*/") {
                None => return (String::new(), text.to_string(), true),
                Some(end) => {
                    in_block_comment = false;
                    i = end + 2;
                    continue;
                }
            }
        }
        let ch = match char_at(text, i) {
            Some(c) => c,
            None => break,
        };
        if ch == '"' {
            i = scan_string(text, i);
            comment_start = None;
            continue;
        }
        if starts_with_at(text, i, "//") {
            comment_start = Some(i);
            break;
        }
        if starts_with_at(text, i, "/*") {
            match find_from(text, i + 2, "*/") {
                None => return (text[..i].trim_end().to_string(), text[i..].to_string(), true),
                Some(end) => {
                    if text[end + 2..].trim().is_empty() {
                        comment_start = Some(i);
                        break;
                    }
                    i = end + 2;
                    comment_start = None;
                    continue;
                }
            }
        }
        if !ch.is_whitespace() {
            comment_start = None;
        }
        i += ch.len_utf8();
    }
    match comment_start {
        Some(start) => (
            text[..start].trim_end().to_string(),
            text[start..].trim_end().to_string(),
            in_block_comment,
        ),
        None => (text.trim_end().to_string(), String::new(), in_block_comment),
    }
}

/// Tokenize a line of code, or return `None` when it cannot be tokenized.
///
/// Balanced `()`/`[]`/`{}` groups become a single token holding their verbatim
/// text; a group left open at the end of the line yields `None`, which the
/// callers treat as "leave this line alone".
pub fn tokenize(code: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let n = code.len();
    while i < n {
        let ch = char_at(code, i)?;
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        let start = i;
        if starts_with_at(code, i, "(*") && !starts_with_at(code, i, "(*)") {
            let end = find_from(code, i + 2, "*)")?;
            i = end + 2;
            tokens.push(Token::new(Kind::Attr, &code[start..i], start, i));
            continue;
        }
        if ch == '"' {
            i = scan_string(code, i);
            tokens.push(Token::new(Kind::Word, &code[start..i], start, i));
            continue;
        }
        if starts_with_at(code, i, "//") || starts_with_at(code, i, "/*") {
            // A comment embedded in code: keep the rest verbatim as one token.
            tokens.push(Token::new(Kind::Word, &code[i..], start, n));
            return Some(tokens);
        }
        if matches!(ch, '(' | '[' | '{') || (ch == '\'' && char_at(code, i + 1) == Some('{')) {
            let (kind, end) = scan_group(code, i)?;
            i = end;
            tokens.push(Token::new(kind, &code[start..i], start, i));
            continue;
        }
        if matches!(ch, ')' | ']' | '}') {
            // An unmatched closer: the line closes a delimiter opened earlier.
            i += 1;
            tokens.push(Token::new(Kind::Punct, &ch.to_string(), start, i));
            continue;
        }
        if starts_with_at(code, i, "::") {
            i += 2;
            tokens.push(Token::new(Kind::Op, "::", start, i));
            continue;
        }
        if matches!(ch, ',' | ';' | ':' | '.') {
            i += 1;
            tokens.push(Token::new(Kind::Punct, &ch.to_string(), start, i));
            continue;
        }
        if is_word_start(ch) {
            i = scan_word(code, i);
            tokens.push(Token::new(Kind::Word, &code[start..i], start, i));
            continue;
        }
        let mut matched = false;
        for operator in OPERATORS {
            if starts_with_at(code, i, operator) {
                i += operator.len();
                tokens.push(Token::new(Kind::Op, operator, start, i));
                matched = true;
                break;
            }
        }
        if !matched {
            return None;
        }
    }
    Some(join_scopes(tokens))
}

fn scan_group(code: &str, mut i: usize) -> Option<(Kind, usize)> {
    if char_at(code, i) == Some('\'') {
        i += 1;
    }
    let kind = match char_at(code, i)? {
        '(' => Kind::Parens,
        '[' => Kind::Brackets,
        '{' => Kind::Braces,
        _ => return None,
    };
    let end = match_group(code, i)?;
    Some((kind, end))
}

fn scan_word(code: &str, i: usize) -> usize {
    let mut i = i;
    let n = code.len();
    if char_at(code, i) == Some('\\') {
        // an escaped identifier runs to the next whitespace
        i += 1;
        while i < n {
            match char_at(code, i) {
                Some(c) if !c.is_whitespace() => i += c.len_utf8(),
                _ => break,
            }
        }
        return i;
    }
    if char_at(code, i) == Some('`') {
        i += 1;
    }
    if let Some(c) = char_at(code, i) {
        i += c.len_utf8();
    }
    while i < n {
        match char_at(code, i) {
            Some(c) if is_word_body(c) => i += c.len_utf8(),
            _ => break,
        }
    }
    i
}

/// Fold `pkg::name` chains into a single word token.
fn join_scopes(tokens: Vec<Token>) -> Vec<Token> {
    let mut joined: Vec<Token> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if matches!(token.kind, Kind::Word | Kind::Brackets) && joined.len() >= 2 {
            let n = joined.len();
            if joined[n - 1].kind == Kind::Op && joined[n - 1].text == "::" && joined[n - 2].kind == Kind::Word {
                let scope = joined.pop().unwrap();
                let base = joined.pop().unwrap();
                let text = format!("{}{}{}", base.text, scope.text, token.text);
                joined.push(Token::new(Kind::Word, &text, base.start, token.end));
                continue;
            }
        }
        joined.push(token);
    }
    joined
}

/// Return the index just past the balanced group opening at `i`, or `None`.
fn match_group(code: &str, mut i: usize) -> Option<usize> {
    let mut stack: Vec<char> = Vec::new();
    let n = code.len();
    while i < n {
        let ch = char_at(code, i)?;
        if ch == '"' {
            i = scan_string(code, i);
            continue;
        }
        if starts_with_at(code, i, "//") {
            return None;
        }
        if starts_with_at(code, i, "/*") {
            let end = find_from(code, i + 2, "*/")?;
            i = end + 2;
            continue;
        }
        match ch {
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            ')' | ']' | '}' => {
                if stack.last() != Some(&ch) {
                    return None;
                }
                stack.pop();
                if stack.is_empty() {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += ch.len_utf8();
    }
    None
}

/// Net change in ()/[]/{} nesting contributed by `code`.
pub fn depth_delta(code: &str) -> i32 {
    let mut delta = 0i32;
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
            '(' | '[' | '{' => delta += 1,
            ')' | ']' | '}' => delta -= 1,
            _ => {}
        }
        i += ch.len_utf8();
    }
    delta
}

// ---------------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------------

static FORMAT_OFF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"//\s*verilog-format\s*:\s*off\b").unwrap());
static FORMAT_ON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"//\s*verilog-format\s*:\s*on\b").unwrap());

/// One physical source line, split into code and trailing comment.
#[derive(Clone, Debug)]
pub struct Line {
    pub raw: String,
    pub indent: String,
    pub code: String,
    pub comment: String,
    /// whitespace between the code and its trailing comment, as written
    pub gap: String,
    pub tokens: Vec<Token>,
    /// depth of open ()/[]/{} before this line's first character
    pub depth: i32,
    /// True when the line lies inside a multi-line block comment
    pub in_block_comment: bool,
    /// True when the line lies in a `verilog-format: off` region
    pub protected: bool,
    /// True for `` `define ``/`` `ifdef ``/... lines
    pub directive: bool,
}

impl Line {
    pub fn blank(&self) -> bool {
        self.code.is_empty() && self.comment.is_empty()
    }

    pub fn comment_only(&self) -> bool {
        self.code.is_empty() && !self.comment.is_empty()
    }

    pub fn render(&self, indent: &str, code: &str, comment: &str) -> String {
        let mut text = format!("{indent}{code}");
        if !comment.is_empty() {
            text = if !code.is_empty() {
                format!("{text}{}{comment}", self.gap)
            } else {
                format!("{indent}{comment}")
            };
        }
        text.trim_end().to_string()
    }
}

/// Split `text` into annotated [`Line`] records.
pub fn scan_lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut in_block_comment = false;
    let mut protected = false;
    let mut depth = 0i32;
    for raw in text.split('\n') {
        let was_in_block_comment = in_block_comment;
        let stripped = raw.trim();
        let (code, comment, still_in_block_comment) = split_comment(stripped, was_in_block_comment);
        in_block_comment = still_in_block_comment;
        let indent = if stripped.is_empty() {
            String::new()
        } else {
            let lead_bytes = raw.len() - raw.trim_start().len();
            raw[..lead_bytes].to_string()
        };
        let mut gap = " ".to_string();
        if !code.is_empty() && !comment.is_empty() {
            let start = code.len();
            let end = stripped.len() - comment.len();
            let g = &stripped[start..end];
            gap = if g.is_empty() { " ".to_string() } else { g.to_string() };
        }
        let directive = code.starts_with('`');
        let mut tokens = Vec::new();
        let line_depth = depth; // depth *before* this line's own delta, matching Python's ordering
        if !was_in_block_comment {
            tokens = tokenize(&code).unwrap_or_default();
            depth += depth_delta(&code);
        }
        let mut line = Line {
            raw: raw.to_string(),
            indent,
            code,
            comment,
            gap,
            tokens,
            depth: line_depth,
            in_block_comment: was_in_block_comment,
            protected: false,
            directive,
        };
        if protected {
            line.protected = true;
            if FORMAT_ON.is_match(raw) {
                protected = false;
            }
        } else if FORMAT_OFF.is_match(raw) {
            line.protected = true;
            protected = true;
        }
        lines.push(line);
    }
    lines
}

/// Flatten a line of code to the atoms of the whitespace-insensitive token
/// stream: an escaped identifier (which runs to whitespace, so the space that
/// ends it is not free to move), a string literal, an `(* attr *)`, a run of
/// identifier characters, or any other single non-space character.
fn flat_atoms(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let n = code.len();
    while i < n {
        let ch = match char_at(code, i) {
            Some(c) => c,
            None => break,
        };
        if ch == '\\' {
            let start = i;
            i += ch.len_utf8();
            while i < n {
                match char_at(code, i) {
                    Some(c) if !c.is_whitespace() => i += c.len_utf8(),
                    _ => break,
                }
            }
            out.push(code[start..i].to_string());
            continue;
        }
        if ch == '"' {
            let start = i;
            i = scan_string(code, i);
            out.push(code[start..i].to_string());
            continue;
        }
        if starts_with_at(code, i, "(*")
            && !starts_with_at(code, i, "(*)")
            && let Some(end) = find_from(code, i + 2, "*)")
        {
            let start = i;
            i = end + 2;
            out.push(code[start..i].to_string());
            continue;
        }
        if is_flat_atom_char(ch) {
            let start = i;
            while i < n {
                match char_at(code, i) {
                    Some(c) if is_flat_atom_char(c) => i += c.len_utf8(),
                    _ => break,
                }
            }
            out.push(code[start..i].to_string());
            continue;
        }
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        out.push(ch.to_string());
        i += ch.len_utf8();
    }
    out
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Flatten `text` to a whitespace- and newline-insensitive token stream.
///
/// Delimiters are not folded into groups here, so a token that legitimately
/// changes lines -- the `#(` a header reflow pulls up -- still compares
/// equal, while a lost, gained or altered token still shows up.
pub fn token_stream(text: &str) -> Vec<String> {
    let mut stream = Vec::new();
    for line in scan_lines(text) {
        stream.extend(flat_atoms(&line.code));
        let collapsed = collapse_whitespace(&line.comment);
        if !collapsed.is_empty() {
            stream.push(collapsed);
        }
    }
    stream
}
