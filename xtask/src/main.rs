//! Pearl's project tasks, run with `cargo xtask <task>`:
//!
//! - `ci`: every quality gate before a commit: fmt, clippy (warnings are errors), tests, assets,
//!   panics. Stops at the first that fails.
//! - `assets`: every asset file has its row in `ATTRIBUTION.md`.
//! - `panics`: the crate's roots deny unwrap, expect, panic, todo, unimplemented and unreachable
//!   outside tests, and forbid unsafe code.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Files under these folders, at any depth, are assets: each needs a row in ATTRIBUTION.md.
const ASSET_DIRS: [&str; 2] = ["assets", "docs/brand"];
/// The attributes every root carries.
const ROOT_ATTRIBUTES: [&str; 2] = [
    "#![forbid(unsafe_code)]",
    "#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]",
];
const ROOTS: [&str; 2] = ["src/lib.rs", "src/main.rs"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

fn asset_files(dir: &Path, into: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            asset_files(&path, into)?;
        } else {
            into.push(path);
        }
    }
    Ok(())
}

fn assets() -> Result<(), String> {
    let root = root();
    let attribution = std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut files = Vec::new();
    for dir in ASSET_DIRS {
        // Without the folder nothing would be checked: that's a failure, not a pass.
        asset_files(&root.join(dir), &mut files)?;
    }
    let mut missing = Vec::new();
    for file in files {
        let name = file.strip_prefix(&root).unwrap_or(&file).display().to_string().replace('\\', "/");
        // A row names the file in backticks.
        if !attribution.contains(&format!("`{name}`")) {
            missing.push(name);
        }
    }
    if missing.is_empty() { Ok(()) } else { Err(format!("assets without a row in ATTRIBUTION.md: {}", missing.join(", "))) }
}

fn panics() -> Result<(), String> {
    let root = root();
    let mut missing = Vec::new();
    for file in ROOTS {
        let source = std::fs::read_to_string(root.join(file)).map_err(|e| format!("{file}: {e}"))?;
        missing.extend(ROOT_ATTRIBUTES.iter().filter(|a| !source.contains(**a)).map(|a| format!("{file} lacks {a}")));
    }
    if missing.is_empty() { Ok(()) } else { Err(missing.join("\n")) }
}

/// One gate: it passes, or says why not.
type Task = fn() -> Result<(), String>;

/// Runs cargo with these arguments in the workspace.
fn cargo(args: &[&str]) -> Result<(), String> {
    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(program).args(args).current_dir(root()).status().map_err(|e| format!("cargo {}: {e}", args.join(" ")))?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

fn ci() -> Result<(), String> {
    let steps: [(&str, Task); 5] = [
        ("fmt", || cargo(&["fmt", "--all", "--check"])),
        ("clippy", || cargo(&["clippy", "--quiet", "--workspace", "--all-targets", "--", "-D", "warnings"])),
        ("tests", || cargo(&["test", "--quiet", "--workspace"])),
        ("assets", assets),
        ("panics", panics),
    ];
    for (name, step) in steps {
        eprintln!("xtask ci: {name}");
        step().map_err(|e| format!("{name}: {e}"))?;
    }
    eprintln!("xtask ci: every gate passed");
    Ok(())
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "ci" => ci(),
        "assets" => assets(),
        "panics" => panics(),
        _ => Err("usage: cargo xtask <ci|assets|panics>".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
