//! Writes THIRD-PARTY-LICENSES.txt from the licence files of the packages that build Pearl, for every
//! platform: `cargo run --example licenses`. A test fails when the file falls behind Cargo.lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const OUTPUT: &str = "THIRD-PARTY-LICENSES.txt";
/// Where a package's licence files aren't: fixtures there have terms of their own, for files that
/// don't ship in a program.
const NOT_SHIPPED: [&str; 6] = ["tests", "test", "examples", "benches", "target", ".git"];
const NOT_LICENCE_TEXT: [&str; 14] = ["rs", "py", "toml", "json", "yml", "yaml", "png", "svg", "ttf", "otf", "c", "h", "js", "css"];

fn is_licence_name(name: &str) -> bool {
    let name = name.to_lowercase();
    let text = Path::new(&name).extension().is_none_or(|e| !NOT_LICENCE_TEXT.contains(&e.to_string_lossy().as_ref()));
    text && ["license", "licence", "copying", "notice", "copyright", "unlicense", "ofl"].iter().any(|p| name.starts_with(p))
}

fn licence_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !NOT_SHIPPED.contains(&name.as_str()) {
                licence_files(&path, into);
            }
        } else if is_licence_name(&name) {
            into.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n").trim().to_string()
}

fn rule(title: &str) -> String {
    format!("\n\n{}\n{title}\n{}\n\n", "=".repeat(80), "=".repeat(80))
}

/// The packages reached through normal and build dependencies, by name and version.
fn packages(metadata: &Value) -> Vec<&Value> {
    let by_id: BTreeMap<&str, &Value> = metadata["packages"].as_array().unwrap().iter().map(|p| (p["id"].as_str().unwrap(), p)).collect();
    let nodes: BTreeMap<&str, &Value> = metadata["resolve"]["nodes"].as_array().unwrap().iter().map(|n| (n["id"].as_str().unwrap(), n)).collect();
    let root = metadata["resolve"]["root"].as_str().unwrap();
    let mut seen = vec![root];
    let mut at = 0;
    while at < seen.len() {
        for dep in nodes[seen[at]]["deps"].as_array().unwrap() {
            let shipped = dep["dep_kinds"].as_array().unwrap().iter().any(|k| k["kind"].is_null() || k["kind"] == "build");
            let id = dep["pkg"].as_str().unwrap();
            if shipped && !seen.contains(&id) {
                seen.push(id);
            }
        }
        at += 1;
    }
    let mut out: Vec<&Value> = seen[1..].iter().map(|id| by_id[id]).collect();
    out.sort_by_key(|p| (p["name"].as_str().unwrap(), p["version"].as_str().unwrap()));
    out
}

/// The whole file, given the project's folder.
pub fn generate(project: &Path) -> String {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--offline", "--locked"])
        .current_dir(project)
        .output()
        .expect("cargo metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata's output");
    let packages = packages(&metadata);

    let mut out = String::from("Pearl and what it is made of\n\n");
    out += &read(&project.join("NOTICE"));

    out += &rule("Fonts in Pearl's interface: Inter (SIL Open Font License 1.1)");
    out += &read(&project.join("assets/fonts/OFL-Inter.txt"));
    out += &rule("Fonts in Pearl's interface: JetBrains Mono (SIL Open Font License 1.1)");
    out += &read(&project.join("assets/fonts/OFL-JetBrainsMono.txt"));

    out += &rule("Packages");
    let mut texts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut without = Vec::new();
    for p in &packages {
        let (name, version) = (p["name"].as_str().unwrap(), p["version"].as_str().unwrap());
        let licence = p["license"].as_str().unwrap_or("see its licence file");
        out += &format!("{name} {version}: {licence}\n");
        let dir = Path::new(p["manifest_path"].as_str().unwrap()).parent().unwrap();
        let mut files = Vec::new();
        licence_files(dir, &mut files);
        if name == "epaint_default_fonts"
            && let Ok(fonts) = std::fs::read_dir(dir.join("fonts"))
        {
            files.extend(fonts.flatten().map(|e| e.path()).filter(|f| f.extension().is_some_and(|e| e == "txt")));
        }
        if let Some(named) = p["license_file"].as_str() {
            files.push(dir.join(named));
        }
        files.sort();
        files.dedup();
        if files.is_empty() {
            let authors: Vec<&str> = p["authors"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            without.push(format!(
                "{name} {version} ({licence}), by {}",
                if authors.is_empty() { "its contributors".to_string() } else { authors.join(", ") }
            ));
        }
        for file in files {
            let from = file.strip_prefix(dir).unwrap().display().to_string().replace('\\', "/");
            texts.entry(read(&file)).or_default().push(format!("{name} {version} ({from})"));
        }
    }

    // Each text once, under everything it came with, in an order that doesn't depend on the text.
    let mut sections: Vec<(Vec<String>, String)> = texts
        .into_iter()
        .map(|(text, mut from)| {
            from.sort();
            (from, text)
        })
        .collect();
    sections.sort();
    for (from, text) in sections {
        out += &rule(&format!("From:\n{}", from.join("\n")));
        out += &text;
    }

    out += &rule("Published without a licence file");
    out += "These packages name their licences in their manifests but publish no licence file. The\n";
    out += "standard texts of the MIT and Apache licences follow the list. The authors are as the manifests\n";
    out += "give them.\n\n";
    out += &without.join("\n");
    let mit = read(&project.join("LICENSE-MIT"));
    let permission = mit.find("Permission is hereby granted").expect("the MIT licence's text");
    out += &rule("MIT License, standard text");
    out += &mit[permission..];
    out += &rule("Apache License 2.0, standard text");
    out += &read(&project.join("LICENSE-APACHE"));
    out.push('\n');
    out
}

fn main() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::write(project.join(OUTPUT), generate(&project)).expect(OUTPUT);
    println!("{OUTPUT}");
}
