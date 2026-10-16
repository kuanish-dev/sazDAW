//! Architecture guard: layers know only the contracts, and only `daw-bootstrap` assembles the implementations.
//! The check reads the crates' Cargo.toml, so a violation is caught before the first run.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// `daw-*` dependencies from the `[dependencies]` section and `[dependencies.daw-…]` tables (dev dependencies for tests do not count).
fn runtime_daw_dependencies(manifest: &Path) -> Vec<String> {
    let text = fs::read_to_string(manifest).unwrap();
    let mut in_dependencies = false;
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]";
            if let Some(name) =
                line.strip_prefix("[dependencies.").and_then(|rest| rest.strip_suffix(']')).filter(|n| n.starts_with("daw-"))
            {
                found.push(name.to_string());
            }
        } else if in_dependencies && line.starts_with("daw-") {
            found.push(line.split(['=', ' ']).next().unwrap().to_string());
        }
    }
    found
}

fn crate_manifests() -> Vec<(String, PathBuf)> {
    let root = workspace_root();
    let mut list: Vec<_> = fs::read_dir(root.join("crates"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().join("Cargo.toml")))
        .filter(|(_, manifest)| manifest.is_file())
        .collect();
    list.push(("daw-desktop".into(), root.join("app/src-tauri/Cargo.toml")));
    list
}

#[test]
fn implementation_crates_depend_only_on_contracts() {
    let roots_and_wiring = ["daw-bootstrap", "daw-app", "daw-desktop"];
    for (name, manifest) in crate_manifests() {
        if name == "daw-contracts" || roots_and_wiring.contains(&name.as_str()) {
            continue;
        }
        let deps = runtime_daw_dependencies(&manifest);
        // The MCP protocol is a layer over RPC: it also sees `daw-rpc` (the method table), but not the implementations.
        let allowed: &[&str] = if name == "daw-mcp" { &["daw-contracts", "daw-rpc"] } else { &["daw-contracts"] };
        assert!(
            deps.iter().all(|d| allowed.contains(&d.as_str())),
            "{name} depends on implementations {deps:?}: a layer must know only {allowed:?}"
        );
    }
}

#[test]
fn the_manifest_parser_sees_inline_and_table_dependencies_but_not_dev_ones() {
    let manifest = std::env::temp_dir().join(format!("daw-arch-manifest-{}.toml", std::process::id()));
    fs::write(
        &manifest,
        "[dependencies]\ndaw-contracts = { path = \"x\" }\n\n[dependencies.daw-engine]\npath = \"y\"\n\n[dev-dependencies]\ndaw-bootstrap = { path = \"z\" }\n",
    )
    .unwrap();
    assert_eq!(runtime_daw_dependencies(&manifest), ["daw-contracts", "daw-engine"]);
    fs::remove_file(manifest).ok();
}

#[test]
fn contracts_depend_on_nothing_of_ours() {
    let manifest = workspace_root().join("crates/daw-contracts/Cargo.toml");
    assert_eq!(runtime_daw_dependencies(&manifest), [] as [std::string::String; 0]);
}

#[test]
fn application_roots_see_only_bootstrap_rpc_and_contracts() {
    for (name, manifest) in crate_manifests() {
        if name != "daw-app" && name != "daw-desktop" {
            continue;
        }
        let deps = runtime_daw_dependencies(&manifest);
        assert!(
            deps.iter().all(|d| ["daw-contracts", "daw-bootstrap", "daw-rpc", "daw-mcp"].contains(&d.as_str())),
            "root {name} pulls in implementations directly: {deps:?}; only daw-bootstrap may assemble them (daw-rpc and the daw-mcp protocol are allowed too)"
        );
    }
}

/// `unsafe` is allowed only in `daw-vst3` (the VST3 C interfaces): all other crates inherit the strict workspace rules.
#[test]
fn only_vst3_crate_may_opt_out_of_workspace_lints() {
    for (name, manifest) in crate_manifests() {
        let text = fs::read_to_string(&manifest).unwrap();
        let inherits = text.lines().any(|line| line.trim() == "workspace = true") && text.contains("[lints]");
        if name == "daw-vst3" {
            assert!(!inherits, "daw-vst3 sets its own lint rules (unsafe_code = allow)");
        } else {
            assert!(inherits, "{name} must inherit [lints] workspace = true: unsafe is allowed only in daw-vst3");
        }
    }
}

/// The `rule = level` pairs of a manifest's `[section]` (comments and empty lines are skipped).
fn lint_table(manifest: &Path, section: &str) -> BTreeMap<String, String> {
    let text = fs::read_to_string(manifest).unwrap();
    let header = format!("[{section}]");
    let mut inside = false;
    let mut table = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            let (key, value) = line.split_once('=').unwrap_or_else(|| panic!("{}: odd line \"{line}\"", manifest.display()));
            table.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    table
}

/// The `daw-vst3` rules are a copy of the workspace rules (they cannot be inherited because of `unsafe_code = "forbid"`);
/// the copy must not drift from the original. The only differences: `unsafe` is allowed, and every `unsafe` block is
/// documented and does one thing.
#[test]
fn vst3_lints_are_the_workspace_lints_plus_unsafe_rules() {
    let root = workspace_root();
    let (workspace, vst3) = (root.join("Cargo.toml"), root.join("crates/daw-vst3/Cargo.toml"));

    let mut rust = lint_table(&workspace, "workspace.lints.rust");
    assert_eq!(rust.insert("unsafe_code".into(), "\"allow\"".into()).as_deref(), Some("\"forbid\""));
    assert_eq!(lint_table(&vst3, "lints.rust"), rust, "[lints.rust] of daw-vst3 drifted from the workspace");

    let mut clippy = lint_table(&workspace, "workspace.lints.clippy");
    assert!(!clippy.is_empty(), "the workspace has no [workspace.lints.clippy]");
    for extra in ["undocumented_unsafe_blocks", "multiple_unsafe_ops_per_block"] {
        clippy.insert(extra.into(), "\"warn\"".into());
    }
    assert_eq!(lint_table(&vst3, "lints.clippy"), clippy, "[lints.clippy] of daw-vst3 drifted from the workspace");
}
