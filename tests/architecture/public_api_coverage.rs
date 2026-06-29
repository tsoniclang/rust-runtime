use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn public_functions_have_executable_test_references() {
    assert_public_functions_have_test_references();
}

fn assert_public_functions_have_test_references() {
    let root = locate_workspace_root().expect("workspace root");
    let coverage_text = collect_coverage_text(&root);
    let mut missing = Vec::new();

    for (path, function_name) in public_functions(&root) {
        if !contains_word(&coverage_text, &function_name) {
            missing.push(format!("{}::{function_name}", path.display()));
        }
    }

    if !missing.is_empty() {
        panic!(
            "public functions without executable test references:\n - {}",
            missing.join("\n - ")
        );
    }
}

fn locate_workspace_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut current = manifest_dir.to_path_buf();
    loop {
        if current.join("Cargo.toml").exists() && current.join("crates").is_dir() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
}

fn public_functions(root: &Path) -> Vec<(PathBuf, String)> {
    let mut result = Vec::new();
    for file in rust_files_under(&root.join("crates")) {
        let source = fs::read_to_string(&file).expect("source file");
        for line in source.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("pub fn ") else {
                continue;
            };
            let name = rest
                .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
                .next()
                .unwrap_or_default();
            if !name.is_empty() {
                result.push((file.clone(), name.to_string()));
            }
        }
    }
    result
}

fn collect_coverage_text(root: &Path) -> String {
    let mut text = String::new();
    for file in rust_files_under(&root.join("tests"))
        .into_iter()
        .chain(rust_files_under(&root.join("crates")))
    {
        if file.file_name().and_then(|name| name.to_str()) == Some("public_api_coverage.rs") {
            continue;
        }
        let mut source = fs::read_to_string(file).expect("coverage file");
        for (_, function_name) in public_functions(root) {
            source = remove_function_definition_lines(&source, &function_name);
        }
        text.push_str(&source);
        text.push('\n');
    }
    text
}

fn remove_function_definition_lines(source: &str, function_name: &str) -> String {
    source
        .lines()
        .filter(|line| {
            !line
                .trim_start()
                .strip_prefix("pub fn ")
                .is_some_and(|rest| {
                    rest.starts_with(function_name)
                        && !is_ident(rest.chars().nth(function_name.len()))
                })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(index, _)| {
        let before = haystack[..index].chars().next_back();
        let after = haystack[index + needle.len()..].chars().next();
        !is_ident(before) && !is_ident(after)
    })
}

fn is_ident(ch: Option<char>) -> bool {
    matches!(
        ch,
        Some('_') | Some('a'..='z') | Some('A'..='Z') | Some('0'..='9')
    )
}
