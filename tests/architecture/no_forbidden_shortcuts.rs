use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_PATTERNS: &[&str] = &[
    "quickjs",
    "rquickjs",
    "v8",
    "boa_engine",
    "std::process::Command::new(\"node\")",
    "std::process::Command::new(\"npm\")",
    "std::process::Command::new(\"npx\")",
    "std::process::Command::new(\"tsx\")",
    "Command::new(\"node\")",
    "Command::new(\"npm\")",
    "Command::new(\"npx\")",
    "Command::new(\"tsx\")",
    "std::any::Any",
    "TypeId",
    "downcast",
];

#[test]
fn no_forbidden_shortcuts_present_in_product_sources() {
    let workspace_root = locate_workspace_root()
        .unwrap_or_else(|| panic!("unable to locate workspace root from test manifest directory"));
    let mut violations = Vec::new();
    let mut rust_files = Vec::new();
    collect_workspace_product_sources(&workspace_root, &mut rust_files);

    for file in rust_files {
        let source = read_source_file(&file);
        for forbidden in find_forbidden_patterns(&source) {
            violations.push(format!("{}: contains `{}`", file.display(), forbidden));
        }
    }

    if !violations.is_empty() {
        let mut message = String::from("forbidden shortcuts found:\n");
        for violation in violations {
            message.push_str(" - ");
            message.push_str(&violation);
            message.push('\n');
        }
        panic!("{}", message);
    }
}

#[test]
fn no_forbidden_shortcuts_in_fixture_text() {
    let source = r#"
        let code = std::process::Command::new("node").arg("--version").spawn();
    "#;
    let hits = find_forbidden_patterns(source);
    assert!(!hits.is_empty());
}

#[test]
fn allowlisted_name_occurrences_are_not_flagged_by_scanner() {
    let source = r#"
        use tsonic_rust_node::error::NodeError;
        let kind = "node";
        let module = "tsonic_rust_node";
        let node_error = NodeError::new("E001", "node sample");
        let class = "NodeError";
        assert!(!kind.is_empty() && !module.is_empty() && !class.is_empty());
        assert!(!node_error.code().is_empty());
    "#;
    let hits = find_forbidden_patterns(source);
    assert!(hits.is_empty());
}

fn find_forbidden_patterns(source: &str) -> Vec<&'static str> {
    FORBIDDEN_PATTERNS
        .iter()
        .copied()
        .filter(|pattern| source.contains(pattern))
        .collect()
}

fn locate_workspace_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut current = manifest_dir.to_path_buf();
    loop {
        if current.join("Cargo.toml").exists() && current.join("crates").is_dir() {
            return Some(current);
        }

        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
            continue;
        }
        return None;
    }
}

fn collect_workspace_product_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let crates_root = root.join("crates");
    let Ok(crate_entries) = fs::read_dir(&crates_root) else {
        return;
    };

    for entry in crate_entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let src_root = entry_path.join("src");
        if !src_root.is_dir() {
            continue;
        }
        collect_rs_under_dir(&src_root, out);
    }
}

fn collect_rs_under_dir(root: &Path, out: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }

            if entry_path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            out.push(entry_path);
        }
    }
}

fn read_source_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "failed to read Rust source file {}: {}",
            path.display(),
            err
        )
    })
}
