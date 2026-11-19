use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use tsonic_rust_runtime::Record;

#[test]
fn references_preserve_alias_mutations_and_native_integer_values() {
    let values = Record::from_entries([(String::from("large"), Some(9_007_199_254_740_993_u64))]);
    let alias = values.clone();
    alias.set(String::from("small"), Some(4));
    assert_eq!(values.get("large"), Some(9_007_199_254_740_993));
    assert_eq!(values.get("small"), Some(4));
    assert_eq!(values.get_or_default("absent"), None);
    values.set(String::from("small"), None);
    assert!(alias.contains_key("small"));
    assert_eq!(alias.get_or_default("small"), None);
    assert!(alias.remove("small"));
    assert!(!values.contains_key("small"));
    assert!(!values.remove("small"));
    assert_eq!(values, alias);
    assert_ne!(values, Record::default());
}

#[test]
fn native_enumeration_and_copy_preserve_values_without_order_emulation() {
    let values = Record::from_entries([(1, "one"), (2, "two")]);
    let target = Record::from_entries([(2, "replaced"), (3, "three")]);
    target.extend(&values);
    target.extend(&target);
    assert_eq!(target.get(&2), "two");
    let mut keys = target.keys();
    keys.sort_unstable();
    assert_eq!(keys, [1, 2, 3]);
    let mut entries = target.entries();
    entries.sort_unstable_by_key(|(key, _)| *key);
    assert_eq!(entries, [(1, "one"), (2, "two"), (3, "three")]);
    let mut selected = target.values();
    selected.sort_unstable();
    assert_eq!(selected, ["one", "three", "two"]);
    assert_eq!(format!("{target:?}"), "Record");
}

#[test]
#[should_panic(expected = "record key is absent")]
fn non_nullable_missing_keys_follow_native_dictionary_failure() {
    Record::<String, u64>::default().get("absent");
}

#[test]
fn cloning_a_reference_never_clones_its_native_table() {
    struct Measured(Rc<Cell<usize>>);
    impl Clone for Measured {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(Rc::clone(&self.0))
        }
    }
    let clones = Rc::new(Cell::new(0));
    let values = Record::from_entries([(1, Measured(Rc::clone(&clones)))]);
    let alias = values.clone();
    assert_eq!(clones.get(), 0);
    alias.get(&1);
    assert_eq!(clones.get(), 1);
    let mut destination = HashMap::with_capacity(1);
    values.copy_entries_to(&mut destination);
    assert_eq!(clones.get(), 2);
    let copied = Record::from_map(destination);
    assert_eq!(clones.get(), 2);
    assert_ne!(copied, values);
    assert!(copied.contains_key(&1));
}

#[test]
fn borrowed_record_views_retain_storage_identity_without_copying_values() {
    struct NeverCloned;
    let original = Record::from_entries([(String::from("present"), NeverCloned)]);
    let alias = original.clone();
    let distinct = Record::from_entries([(String::from("present"), NeverCloned)]);
    assert_eq!(
        original.storage_identity_key(),
        alias.storage_identity_key()
    );
    assert_ne!(
        original.storage_identity_key(),
        distinct.storage_identity_key()
    );
    original.with_entries(|entries| {
        assert!(entries.contains_key("present"));
        assert!(!entries.contains_key("absent"));
    });
    alias.remove("present");
    assert!(original.with_entries(HashMap::is_empty));
}
