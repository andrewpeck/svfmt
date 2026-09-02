//! Small helpers for scanning `&str` by byte offset while measuring width in
//! Unicode scalar values (matching Python's codepoint-indexed `str`).
//!
//! Every offset produced here always lands on a `char` boundary, because the
//! scanners only ever advance by whole characters -- so ordinary byte slicing
//! (`&s[a..b]`) stays safe throughout the crate.

/// The character starting at byte offset `i`, or `None` past the end.
pub fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

/// True when `s[i..]` starts with `pat`.
pub fn starts_with_at(s: &str, i: usize, pat: &str) -> bool {
    s.get(i..).is_some_and(|t| t.starts_with(pat))
}

/// Codepoint count of `s`, i.e. what Python's `len(str)` would report.
pub fn width(s: &str) -> usize {
    s.chars().count()
}

/// Left-justify `s` to `w` columns, like Python's `str.ljust`.
pub fn ljust(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(width(s));
    format!("{s}{}", " ".repeat(pad))
}

/// Right-justify `s` to `w` columns, like Python's `str.rjust`.
pub fn rjust(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(width(s));
    format!("{}{s}", " ".repeat(pad))
}
