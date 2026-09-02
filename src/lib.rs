//! Whitespace-only auto-formatter for (System)Verilog sources.
//!
//! Two independent passes, both of which only ever rewrite whitespace:
//!
//! `indent`
//!     Re-indent lines from a stack of open blocks and delimiters.
//!
//! `align`
//!     Group consecutive similar lines into runs and pad them into columns,
//!     so declarations, port lists, struct members, assignments, named port
//!     connections and trailing comments line up.
//!
//! Neither pass reflows lines, reorders tokens, or edits comment text. The
//! result is checked against the input token stream before anything is
//! written.

mod align;
mod config;
mod format;
mod header;
mod indent;
mod lexer;
mod text;

pub use config::{Config, DEFAULT_SUFFIXES, find_config, is_excluded, iter_sources, read_config};
pub use format::{FormatOptions, SvFormatError, format_default, format_text};
pub use lexer::token_stream;
