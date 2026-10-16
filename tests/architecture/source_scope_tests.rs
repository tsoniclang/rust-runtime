use super::collect_production_sources;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".temp/source-scope-tests");
        fs::create_dir_all(&parent).expect("source scope fixture parent");
        let root = parent.join(format!("{}-{}", std::process::id(), NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&root).expect("unique source scope fixture");
        for (name, source) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().expect("fixture file parent")).expect("fixture directories");
            fs::write(path, source).expect("fixture source");
        }
        Self { root }
    }

    fn sources(&self) -> Result<BTreeMap<String, String>, String> {
        let root = fs::canonicalize(&self.root).expect("canonical fixture root");
        collect_production_sources(&root.join("crates")).map(|sources| sources.into_iter().map(|unit| {
            (unit.path.strip_prefix(&root).expect("fixture-owned source").to_string_lossy().into_owned(), unit.source)
        }).collect())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { fs::remove_dir_all(&self.root).expect("release owned source scope fixture"); }
}

#[test]
fn test_only_external_subtrees_are_not_orphans_and_names_do_not_define_scope() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "#[cfg(test)] mod tests; pub fn root() {}"),
        ("crates/example/src/tests.rs", "mod child; fn test_only_marker() {}"),
        ("crates/example/src/tests/child.rs", "fn test_child_marker() {}"),
        ("crates/example/src/orphan.rs", "fn orphan_production_marker() {}"),
    ]);
    let sources = fixture.sources().expect("classified sources");
    assert_eq!(sources.len(), 2);
    assert!(sources["crates/example/src/lib.rs"].contains("pub fn root"));
    assert!(sources["crates/example/src/orphan.rs"].contains("orphan_production_marker"));
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "mod tests;"),
        ("crates/example/src/tests.rs", "fn production_marker() {}"),
    ]);
    assert!(fixture.sources().unwrap()["crates/example/src/tests.rs"].contains("production_marker"));
}

#[test]
fn inline_test_items_fields_and_unicode_spans_are_removed_without_touching_production() {
    let fixture = Fixture::new(&[("crates/example/src/lib.rs", "\u{feff}#!/native-tool\n#[cfg(test)] mod checks { fn témoin() {} }\n#[cfg(test)] fn hidden() {}\nstruct Kept { #[cfg(test)] hidden: Hidden, value: u8 }\npub fn production_marker() {}")]);
    let source = &fixture.sources().unwrap()["crates/example/src/lib.rs"];
    for excluded in ["témoin", "checks", "hidden", "Hidden"] { assert!(!source.contains(excluded), "{excluded}"); }
    assert!(source.contains("value: u8"));
    assert!(source.contains("production_marker"));
    assert!(source.starts_with('\u{feff}'));
    assert_eq!(source.lines().count(), 5);
}

#[test]
fn any_production_route_keeps_a_shared_file_and_nested_root_names_are_not_roots() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "#[cfg(test)] #[path = \"shared.rs\"] mod tests; #[path = \"shared.rs\"] mod live; #[cfg(test)] #[path = \"checks/lib.rs\"] mod nested;"),
        ("crates/example/src/shared.rs", "fn production_marker() {}"),
        ("crates/example/src/checks/lib.rs", "fn test_only_marker() {}"),
    ]);
    let sources = fixture.sources().unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources["crates/example/src/shared.rs"].contains("production_marker"));
    assert!(!sources.contains_key("crates/example/src/checks/lib.rs"));
}

#[test]
fn production_tokens_in_a_file_named_tests_remain_byte_identical() {
    let input = "pub fn inspect(value: &dyn std::any::Any) { value.downcast_ref::<u8>(); }";
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "mod tests;"),
        ("crates/example/src/tests.rs", input),
    ]);
    assert_eq!(fixture.sources().unwrap()["crates/example/src/tests.rs"], input);
}

#[test]
fn unknown_cfg_remains_production_possible_and_known_test_predicates_are_exact() {
    for condition in ["not(test)", "any(test, feature = \"future\")", "feature = \"future\"", "all(not(test), feature = \"future\")"] {
        let input = format!("#[cfg({condition})] fn production_marker() {{}}");
        let fixture = Fixture::new(&[("crates/example/src/lib.rs", &input)]);
        assert!(fixture.sources().unwrap()["crates/example/src/lib.rs"].contains("production_marker"), "{condition}");
    }
    for condition in ["test", "all(test, feature = \"future\")", "any()", "not(not(test))"] {
        let input = format!("#[cfg({condition})] fn test_only_marker() {{}} pub fn kept() {{}}");
        let fixture = Fixture::new(&[("crates/example/src/lib.rs", &input)]);
        assert!(!fixture.sources().unwrap()["crates/example/src/lib.rs"].contains("test_only_marker"), "{condition}");
    }
}

#[test]
fn native_paths_respect_non_mod_files_inline_paths_and_path_attribute_ownership() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "mod normal; #[path = \"alternate.rs\"] mod selected; #[path = \"folder\"] mod inline { #[path = \"nested.rs\"] mod child; }"),
        ("crates/example/src/normal.rs", "mod deep { #[path = \"leaf.rs\"] mod leaf; }"),
        ("crates/example/src/normal/deep/leaf.rs", "fn normal_marker() {}"),
        ("crates/example/src/alternate.rs", "mod sibling;"),
        ("crates/example/src/sibling.rs", "fn sibling_marker() {}"),
        ("crates/example/src/folder/nested.rs", "fn inline_marker() {}"),
    ]);
    let sources = fixture.sources().unwrap();
    assert_eq!(sources.len(), 6);
    assert!(sources["crates/example/src/sibling.rs"].contains("sibling_marker"));
    assert!(sources["crates/example/src/normal/deep/leaf.rs"].contains("normal_marker"));
}

#[test]
fn static_includes_follow_physical_source_directories_and_keep_production_routes() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "include!(\"pieces/production.rs\"); #[cfg(test)] mod tests { include!(\"pieces/test.rs\"); }"),
        ("crates/example/src/pieces/production.rs", "mod child; include!(\"../../shared.rs\");"),
        ("crates/example/src/pieces/child.rs", "fn production_marker() {}"),
        ("crates/example/shared.rs", "fn outside_src_marker() {}"),
        ("crates/example/src/pieces/test.rs", "fn test_only_marker() {}"),
    ]);
    let sources = fixture.sources().unwrap();
    assert_eq!(sources.len(), 4);
    assert!(sources["crates/example/src/pieces/child.rs"].contains("production_marker"));
    assert!(sources["crates/example/shared.rs"].contains("outside_src_marker"));
    assert!(!sources.contains_key("crates/example/src/pieces/test.rs"));
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "#[cfg(test)] mod tests { include!(\"shared.rs\"); } include!(\"shared.rs\");"),
        ("crates/example/src/shared.rs", "fn production_marker() {}"),
    ]);
    assert!(fixture.sources().unwrap()["crates/example/src/shared.rs"].contains("production_marker"));
}

#[test]
fn expression_and_statement_includes_use_their_native_grammar() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "fn root() -> u8 { include!(\"body.rs\"); include!(\"value.rs\") }"),
        ("crates/example/src/body.rs", "let production_marker = 1;"),
        ("crates/example/src/value.rs", "2"),
    ]);
    let sources = fixture.sources().unwrap();
    assert!(sources["crates/example/src/body.rs"].contains("production_marker"));
    assert_eq!(sources["crates/example/src/value.rs"], "2");
}

#[test]
fn native_cfg_attr_is_transparent_only_when_its_condition_is_known() {
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "#[cfg_attr(test, path = \"missing.rs\")] mod actual; #[cfg_attr(not(test), cfg(test))] fn test_only_marker() {}"),
        ("crates/example/src/actual.rs", "fn production_marker() {}"),
    ]);
    let sources = fixture.sources().unwrap();
    assert!(!sources["crates/example/src/lib.rs"].contains("test_only_marker"));
    assert!(sources["crates/example/src/actual.rs"].contains("production_marker"));
}

#[test]
fn unreadable_ambiguous_dynamic_and_unparseable_sources_fail_closed() {
    for source in [
        "mod missing;",
        "include!(\"missing.rs\");",
        "include!(concat!(\"hidden\", \".rs\"));",
        "#[cfg_attr(feature = \"unknown\", path = \"other.rs\")] mod module;",
        "#[path = concat!(\"other\", \".rs\")] mod module;",
        "#[cfg(not(test, test))] fn invalid() {}",
        "fn invalid( {",
        "macro_rules! hidden { () => { mod unclassified; } }",
    ] {
        let fixture = Fixture::new(&[("crates/example/src/lib.rs", source)]);
        assert!(fixture.sources().is_err(), "{source}");
    }
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "mod ambiguous;"),
        ("crates/example/src/ambiguous.rs", ""),
        ("crates/example/src/ambiguous/mod.rs", ""),
    ]);
    assert!(fixture.sources().is_err());
    let fixture = Fixture::new(&[
        ("crates/example/src/lib.rs", "pub fn root() {}"),
        ("crates/example/src/orphan.rs", "fn invalid( {"),
    ]);
    assert!(fixture.sources().is_err());
}
