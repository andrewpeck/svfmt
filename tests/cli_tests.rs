//! Integration tests for the `svfmt` binary: file discovery, configuration,
//! and the `--check`/`--diff`/`--stdout`/`--quiet` flags.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const MANGLED: &str = "module m;\nlogic a;\nlogic  bb;\nendmodule\n";
const TIDY: &str = "module tidy;\nendmodule\n";
const FORMATTED_MANGLED: &str = "module m;\n  logic a;\n  logic bb;\nendmodule\n";

/// A directory under `target/` that removes itself on drop, so tests don't
/// need an extra crate to get scratch space.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("svfmt-test-{label}-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn svfmt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_svfmt"))
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect()
}

/// `a.sv` and `b.sv` need reformatting, `c.sv` is already tidy.
fn make_sources() -> TempDir {
    let dir = TempDir::new("sources");
    write(dir.path(), "a.sv", MANGLED);
    write(dir.path(), "b.sv", MANGLED);
    write(dir.path(), "c.sv", TIDY);
    dir
}

#[test]
fn cli_reports_each_file_as_it_is_written() {
    let dir = make_sources();
    let output = svfmt().arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let a = dir.path().join("a.sv").display().to_string();
    let b = dir.path().join("b.sv").display().to_string();
    assert_eq!(
        stdout_lines(&output),
        vec![format!("✔ reformatted {a}"), format!("✔ reformatted {b}")]
    );
    assert_eq!(fs::read_to_string(dir.path().join("a.sv")).unwrap(), FORMATTED_MANGLED);
    assert_eq!(fs::read_to_string(dir.path().join("c.sv")).unwrap(), TIDY);
}

#[test]
fn cli_check_reports_without_writing() {
    let dir = make_sources();
    let output = svfmt().arg("--check").arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let a = dir.path().join("a.sv").display().to_string();
    let b = dir.path().join("b.sv").display().to_string();
    assert_eq!(
        stdout_lines(&output),
        vec![format!("✗ would reformat {a}"), format!("✗ would reformat {b}")]
    );
    assert_eq!(fs::read_to_string(dir.path().join("a.sv")).unwrap(), MANGLED);
}

#[test]
fn cli_quiet_is_silent() {
    let dir = make_sources();
    let output = svfmt().arg("--quiet").arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_to_string(dir.path().join("a.sv")).unwrap(), FORMATTED_MANGLED);
}

#[test]
fn cli_diff_writes_nothing() {
    let dir = make_sources();
    let output = svfmt().arg("--diff").arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("+  logic bb;"));
    assert_eq!(fs::read_to_string(dir.path().join("a.sv")).unwrap(), MANGLED);
}

#[test]
fn stdout_prints_without_writing() {
    let dir = make_sources();
    let output = svfmt().arg("--stdout").arg(dir.path().join("a.sv")).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), FORMATTED_MANGLED);
    assert_eq!(fs::read_to_string(dir.path().join("a.sv")).unwrap(), MANGLED);
}

#[test]
fn stdout_prints_an_already_formatted_file() {
    let dir = make_sources();
    write(dir.path(), "a.sv", TIDY);
    let output = svfmt().arg("--stdout").arg(dir.path().join("a.sv")).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), TIDY);
}

#[test]
fn stdout_rejects_several_files() {
    let dir = make_sources();
    let output = svfmt().arg("--stdout").arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exactly one file"));
}

#[test]
fn stdout_rejects_an_excluded_file() {
    // printing nothing would empty the target of a shell redirect
    let dir = make_sources();
    write(dir.path(), "Top/fnp_controller/registers.sv", MANGLED);
    write(
        dir.path(),
        "pyproject.toml",
        "[tool.sv-format]\nexclude = [\"Top/*/registers.sv\"]\n",
    );
    let output = svfmt()
        .current_dir(dir.path())
        .arg("--stdout")
        .arg("Top/fnp_controller/registers.sv")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exactly one file"));
}

#[test]
fn stdout_conflicts_with_check_and_diff() {
    let dir = make_sources();
    let a = dir.path().join("a.sv");
    let out1 = svfmt().arg("--stdout").arg("--check").arg(&a).output().unwrap();
    let out2 = svfmt().arg("--stdout").arg("--diff").arg(&a).output().unwrap();
    assert_eq!(out1.status.code(), Some(2));
    assert_eq!(out2.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out1.stderr).contains("cannot be combined"));
}

// ---------------------------------------------------------------------------------
// configuration
// ---------------------------------------------------------------------------------

/// `Top/fnp_controller/{registers,fnp_controller_top}.sv` and `hdl/core.sv`,
/// all needing reformatting.
fn make_project() -> TempDir {
    let dir = TempDir::new("project");
    write(dir.path(), "Top/fnp_controller/registers.sv", MANGLED);
    write(dir.path(), "Top/fnp_controller/fnp_controller_top.sv", MANGLED);
    write(dir.path(), "hdl/core.sv", MANGLED);
    dir
}

#[test]
fn exclude_from_pyproject() {
    let dir = make_project();
    write(
        dir.path(),
        "pyproject.toml",
        "[tool.sv-format]\nexclude = [\"Top/*/registers.sv\"]\n",
    );
    let output = svfmt().current_dir(dir.path()).arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let reformatted = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(!reformatted.contains("registers.sv"));
    assert!(reformatted.contains("fnp_controller_top.sv"));
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        MANGLED
    );
}

#[test]
fn exclude_applies_to_explicitly_named_files() {
    // pre-commit passes the changed files by name, so they must be skipped too
    let dir = make_project();
    write(
        dir.path(),
        "pyproject.toml",
        "[tool.sv-format]\nexclude = [\"Top/*/registers.sv\"]\n",
    );
    let output = svfmt()
        .current_dir(dir.path())
        .arg("Top/fnp_controller/registers.sv")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        MANGLED
    );
}

#[test]
fn exclude_without_a_slash_matches_any_component() {
    let dir = make_project();
    write(dir.path(), "pyproject.toml", "[tool.sv-format]\nexclude = [\"Top\"]\n");
    let output = svfmt().current_dir(dir.path()).arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        MANGLED
    );
    assert_eq!(fs::read_to_string(dir.path().join("hdl/core.sv")).unwrap(), FORMATTED_MANGLED);
}

#[test]
fn dedicated_config_file_wins() {
    let dir = make_project();
    write(dir.path(), "sv-format.toml", "exclude = [\"hdl\"]\n");
    write(dir.path(), "pyproject.toml", "[tool.sv-format]\nexclude = [\"Top\"]\n");
    let output = svfmt().current_dir(dir.path()).arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(dir.path().join("hdl/core.sv")).unwrap(), MANGLED);
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        FORMATTED_MANGLED
    );
}

#[test]
fn config_is_found_from_a_subdirectory() {
    let dir = make_project();
    write(
        dir.path(),
        "pyproject.toml",
        "[tool.sv-format]\nexclude = [\"Top/*/registers.sv\"]\n",
    );
    let output = svfmt()
        .current_dir(dir.path().join("Top/fnp_controller"))
        .arg("registers.sv")
        .arg("fnp_controller_top.sv")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        MANGLED
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/fnp_controller_top.sv")).unwrap(),
        FORMATTED_MANGLED
    );
}

#[test]
fn exclude_flag_adds_to_the_configured_patterns() {
    let dir = make_project();
    let output = svfmt()
        .current_dir(dir.path())
        .arg("--exclude")
        .arg("core.sv")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(dir.path().join("hdl/core.sv")).unwrap(), MANGLED);
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        FORMATTED_MANGLED
    );
}

#[test]
fn explicit_config_path() {
    // The config file lives in the project root itself, like pytest's shared
    // `tmp_path` fixture does in the original test, so its `exclude`
    // patterns resolve relative to that root.
    let dir = make_project();
    write(dir.path(), "elsewhere.toml", "exclude = [\"Top\"]\n");
    let output = svfmt()
        .arg("--config")
        .arg(dir.path().join("elsewhere.toml"))
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(dir.path().join("Top/fnp_controller/registers.sv")).unwrap(),
        MANGLED
    );
}

#[test]
fn broken_config_is_reported() {
    let dir = make_project();
    write(dir.path(), "pyproject.toml", "[tool.sv-format]\nexclude = \"not-a-list\"\n");
    let output = svfmt().current_dir(dir.path()).arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exclude must be a list of strings"));
}
