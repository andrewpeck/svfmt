//! Settings loaded from a TOML file, and expanding the paths given on the
//! command line into the list of files to format.

use regex::Regex;
use std::path::{Path, PathBuf};

use crate::format::SvFormatError;

pub const DEFAULT_SUFFIXES: [&str; 4] = [".sv", ".svh", ".v", ".vh"];

/// searched upwards from the working directory; the first match wins
pub const CONFIG_FILENAMES: [&str; 3] = ["sv-format.toml", ".sv-format.toml", "pyproject.toml"];

/// table holding the settings, under [tool.<...>] in pyproject.toml
pub const CONFIG_SECTION: &str = "sv-format";

/// Settings loaded from a TOML file, with paths relative to `root`.
#[derive(Debug, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub exclude: Vec<String>,
}

impl Config {
    fn new(root: PathBuf) -> Self {
        Config { root, exclude: Vec::new() }
    }
}

/// Load `path`, or return `None` if it holds no settings for this tool.
pub fn read_config(path: &Path) -> Result<Option<Config>, SvFormatError> {
    let text = std::fs::read_to_string(path).map_err(|error| SvFormatError(format!("{}: {error}", path.display())))?;
    let data: toml::Table = text
        .parse()
        .map_err(|error: toml::de::Error| SvFormatError(format!("{}: {error}", path.display())))?;

    let table: Option<toml::Table> = if path.file_name().is_some_and(|n| n == "pyproject.toml") {
        data.get("tool")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get(CONFIG_SECTION))
            .and_then(|v| v.as_table())
            .cloned()
    } else {
        Some(data)
    };
    let Some(table) = table else {
        return Ok(None);
    };

    let exclude = match table.get("exclude") {
        None => Vec::new(),
        Some(value) => {
            let bad = || SvFormatError(format!("{}: exclude must be a list of strings", path.display()));
            let array = value.as_array().ok_or_else(bad)?;
            array
                .iter()
                .map(|item| item.as_str().map(str::to_string).ok_or_else(bad))
                .collect::<Result<Vec<_>, _>>()?
        }
    };

    let root = path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|error| SvFormatError(format!("{}: {error}", path.display())))?;
    Ok(Some(Config { root, exclude }))
}

/// Search `start` and its parents for the first file that configures us.
pub fn find_config(start: &Path) -> Result<Config, SvFormatError> {
    let resolved = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut dir = Some(resolved.as_path());
    while let Some(directory) = dir {
        for name in CONFIG_FILENAMES {
            let candidate = directory.join(name);
            if candidate.is_file()
                && let Some(config) = read_config(&candidate)?
            {
                return Ok(config);
            }
        }
        dir = directory.parent();
    }
    Ok(Config::new(resolved))
}

/// Translate a `fnmatch`-style glob (`*`, `?`, `[seq]`, `[!seq]`) into an
/// anchored, case-sensitive regex.
fn fnmatch_regex(pattern: &str) -> Regex {
    let mut re = String::from("(?s)^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                re.push_str(".*");
                i += 1;
            }
            '?' => {
                re.push('.');
                i += 1;
            }
            '[' => {
                let mut j = i + 1;
                if j < chars.len() && matches!(chars[j], '!' | '^') {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ']' {
                    j += 1;
                }
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    re.push_str("\\[");
                    i += 1;
                } else {
                    let mut inner: String = chars[i + 1..j].iter().collect();
                    if let Some(rest) = inner.strip_prefix('!') {
                        inner = format!("^{rest}");
                    }
                    re.push('[');
                    re.push_str(&inner);
                    re.push(']');
                    i = j + 1;
                }
            }
            c => {
                re.push_str(&regex::escape(&c.to_string()));
                i += 1;
            }
        }
    }
    re.push('$');
    Regex::new(&re).expect("fnmatch_regex always builds a valid pattern")
}

/// True when `path` matches one of the configured exclude patterns.
///
/// A pattern containing a `/` is matched against the path relative to the
/// configuration file, and against each of its parent directories, so
/// `Top/*/registers.sv` and `build` both work. A pattern without one is
/// matched against every individual path component.
pub fn is_excluded(path: &Path, config: &Config) -> bool {
    if config.exclude.is_empty() {
        return false;
    }
    let Ok(resolved) = path.canonicalize() else { return false };
    let Ok(relative) = resolved.strip_prefix(&config.root) else {
        return false;
    };
    let relative_str = relative.to_string_lossy().replace('\\', "/");

    let mut candidates = vec![relative_str.clone()];
    let mut parts: Vec<&str> = relative_str.split('/').collect();
    while parts.len() > 1 {
        parts.pop();
        candidates.push(parts.join("/"));
    }

    for pattern in &config.exclude {
        if pattern.contains('/') {
            let re = fnmatch_regex(pattern.trim_end_matches('/'));
            if candidates.iter().any(|candidate| re.is_match(candidate)) {
                return true;
            }
        } else {
            let re = fnmatch_regex(pattern);
            if relative_str.split('/').any(|part| re.is_match(part)) {
                return true;
            }
        }
    }
    false
}

fn rglob_files(dir: &Path, suffixes: &[&str]) -> Vec<PathBuf> {
    fn walk(dir: &Path, suffixes: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, suffixes, out);
            } else if path.is_file()
                && let Some(ext) = path.extension()
                && suffixes.contains(&format!(".{}", ext.to_string_lossy()).as_str())
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, suffixes, &mut out);
    out.sort();
    out
}

/// Expand `paths`, recursing into directories and dropping excluded files.
///
/// Exclusions apply to explicitly named files too, so a hook that passes
/// every changed file (pre-commit does) still skips what the configuration
/// ignores.
pub fn iter_sources(paths: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            found.extend(rglob_files(path, &DEFAULT_SUFFIXES));
        } else {
            found.push(path.clone());
        }
    }
    found.into_iter().filter(|path| !is_excluded(path, config)).collect()
}
