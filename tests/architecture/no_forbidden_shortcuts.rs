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
    "Any",
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
        let relative = file
            .strip_prefix(&workspace_root)
            .expect("product source path");
        for forbidden in product_source_violations(relative, &source) {
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
fn native_payload_is_one_sealed_checked_owner() {
    let root = locate_workspace_root().expect("runtime workspace");
    let path = Path::new("crates/tsonic_rust_runtime/src/ts_value.rs");
    let source = read_source_file(&root.join(path));
    assert!(product_source_violations(path, &source).is_empty());
    assert_eq!(source.matches(".downcast").count(), 1);
    assert_eq!(source.matches("Rc::new(PassiveValue(value))").count(), 1);
    assert_eq!(source.matches("Rc::new(IdentityValue(value))").count(), 1);
    assert_eq!(source.matches("Rc::new").count(), 2);
    assert!(!source.contains("unsafe"));
    assert!(!source.contains("Box<"));
}

#[test]
fn checked_native_owners_reject_additional_erasure_and_mutated_contracts() {
    let root = locate_workspace_root().expect("runtime workspace");
    for relative in [
        "crates/tsonic_rust_runtime/src/ts_value.rs",
        "crates/tsonic_rust_runtime/src/object_identity.rs",
        "crates/tsonic_rust_runtime/src/retained_error.rs",
    ] {
        let path = Path::new(relative);
        let source = read_source_file(&root.join(path));
        assert!(product_source_violations(path, &source).is_empty());
        for injected in [
            "use core::any::Any;",
            "use std::{any::Any, fmt};",
            "use core::{any::{Any as Erased}, fmt};",
            "value.downcast_ref::<String>();",
            "let identifier: core::any::TypeId;",
        ] {
            let mutated = format!("{source}\n{injected}");
            assert!(!product_source_violations(path, &mutated).is_empty());
        }
    }
    let path = Path::new("crates/tsonic_rust_runtime/src/ts_value.rs");
    let source = read_source_file(&root.join(path));
    for mutated in [
        source.replace("trait ClosedTsValue {", "pub trait ClosedTsValue {"),
        source.replace(
            "struct PassiveValue<Value>",
            "pub struct PassiveValue<Value>",
        ),
        source.replace("#[repr(transparent)]", "#[repr(C)]"),
        source.replace("downcast_ref::<Payload>()", "downcast_ref::<u64>()"),
    ] {
        assert!(!product_source_violations(path, &mutated).is_empty());
    }
}

#[test]
fn any_import_spellings_and_unowned_recovery_are_rejected() {
    for source in [
        "use std::any::Any;",
        "use core::any::Any;",
        "use core::{any::Any, fmt};",
        "use std::{any::{Any as Native}, fmt};",
        "use core::any::{type_name, Any as Native};",
        "use core::any::*;",
        "use core::any as native;",
        "use core::{any::{*}, fmt};",
        "use std::{any as native, fmt};",
        "fn reflect(value: &dyn Any) {}",
        "value.downcast_ref::<u64>()",
    ] {
        assert!(!find_forbidden_patterns(source).is_empty());
        assert!(!product_source_violations(Path::new("unowned.rs"), source).is_empty());
    }
    assert!(find_forbidden_patterns("let Any_value = 1;").is_empty());
    assert!(find_forbidden_patterns("enum Property { Any, Assigned }").is_empty());
    assert!(find_forbidden_patterns("Property::Any").is_empty());
    assert!(find_forbidden_patterns("fn valid(value: &dyn Anything) {}").is_empty());
    assert!(find_forbidden_patterns("fn valid<Value: AnySuffix>() {}").is_empty());
    assert!(find_forbidden_patterns("company::Any").is_empty());
    assert!(find_forbidden_patterns("/// Any ByteSet may match a single char.").is_empty());
    assert!(find_forbidden_patterns("core::any::type_name::<u64>()").is_empty());
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
        .filter(|pattern| {
            if *pattern == "Any" {
                contains_native_any(source)
            } else {
                source.contains(pattern)
            }
        })
        .collect()
}

fn contains_native_any(source: &str) -> bool {
    let compact: String = source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    compact.contains("any::*")
        || compact.contains("::anyas")
        || compact.contains("{anyas")
        || compact.contains(",anyas")
        || compact.split("any::{").skip(1).any(|group| {
            group
                .split('}')
                .next()
                .unwrap_or_default()
                .split(',')
                .any(|item| item == "*")
        })
        || source.match_indices("Any").any(|(index, _)| {
            let before = &source[..index];
            let after = &source[index + "Any".len()..];
            let identifier = |character: char| character.is_alphanumeric() || character == '_';
            if before.chars().next_back().is_some_and(identifier)
                || after.chars().next().is_some_and(identifier)
            {
                return false;
            }
            let prefix: String = before
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            let namespace = prefix
                .strip_suffix("any::")
                .is_some_and(|owner| !owner.chars().next_back().is_some_and(identifier));
            namespace
                || prefix.ends_with("dyn")
                || prefix.ends_with('+')
                || (prefix.ends_with(':') && !prefix.ends_with("::"))
                || prefix
                    .rsplit_once("any::{")
                    .is_some_and(|(_, group)| !group.contains('}'))
        })
}

fn product_source_violations(path: &Path, source: &str) -> Vec<&'static str> {
    let fragments: &[(&str, usize)] = match path.to_str() {
        Some("crates/tsonic_rust_runtime/src/ts_value.rs") => {
            if !source
                .contains("#[repr(transparent)]\npub struct NativePayload(Rc<dyn ClosedTsValue>);")
                || source.contains("pub trait ClosedTsValue")
                || source.contains("pub struct PassiveValue")
                || source.contains("pub struct IdentityValue")
            {
                return vec!["sealed native payload contract"];
            }
            &[
                (
                    "trait ClosedTsValue {\n    fn native_value(&self) -> &dyn core::any::Any;",
                    1,
                ),
                (
                    r#"impl<Value: 'static> ClosedTsValue for PassiveValue<Value> {
    fn native_value(&self) -> &dyn core::any::Any {
        &self.0
    }
}"#,
                    1,
                ),
                (
                    r#"impl<Value: ObjectIdentityCarrier + 'static> ClosedTsValue for IdentityValue<Value> {
    fn native_value(&self) -> &dyn core::any::Any {
        &self.0
    }

    fn identity_key(&self) -> Option<usize> {
        Some(self.0.object_identity_key())
    }
}"#,
                    1,
                ),
                (
                    r#"pub fn native_value<Payload: Clone + 'static>(&self) -> Option<Payload> {
        self.0.native_value().downcast_ref::<Payload>().cloned()
    }"#,
                    1,
                ),
            ]
        }
        Some("crates/tsonic_rust_runtime/src/object_identity.rs") => &[(
            r#"fn project_native(self: Rc<Self>, output: &mut dyn core::any::Any)
    where
        Self: 'static,
    {
        if let Some(output) = output.downcast_mut::<Option<Rc<Self>>>() {
            *output = Some(self);
        }
    }"#,
            1,
        )],
        Some("crates/tsonic_rust_runtime/src/retained_error.rs") => &[
            ("use core::{any::Any, fmt};", 1),
            (
                r#"pub trait RetainedErrorObject: ErrorObject + ErrorStack {
    fn project_error(self: Rc<Self>, output: &mut dyn Any)
    where
        Self: 'static;
}"#,
                1,
            ),
            (
                r#"pub fn project_error(&self, output: &mut dyn Any) {
        match self {
            Self::Project(error) => error.clone().project_error(output),
            Self::WritableProject(error) => error.clone().project_error(output),
            Self::Native(_) | Self::Runtime(_) | Self::Created(_) => {}
        }
    }"#,
                1,
            ),
            (
                r#"pub fn into_project_error(self, output: &mut dyn Any) {
        match self {
            Self::Project(error) => error.project_error(output),
            Self::WritableProject(error) => error.project_error(output),
            Self::Native(_) | Self::Runtime(_) | Self::Created(_) => {}
        }
    }"#,
                1,
            ),
            (
                r#"pub fn project_error(&self, output: &mut dyn Any) {
        if let Self::Project(error) = self {
            error.clone().project_error(output);
        }
    }"#,
                1,
            ),
            (
                r#"pub fn into_project_error(self, output: &mut dyn Any) {
        if let Self::Project(error) = self {
            error.project_error(output);
        }
    }"#,
                1,
            ),
        ],
        _ => &[],
    };
    let mut remaining = source.to_owned();
    for (fragment, expected) in fragments {
        if remaining.matches(fragment).count() != *expected {
            return vec!["exact checked native projection contract"];
        }
        remaining = remaining.replace(fragment, "");
    }
    find_forbidden_patterns(&remaining)
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
