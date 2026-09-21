use core::convert::Infallible;
use core::cell::Cell;
use tsonic_rust_runtime::{Field, FieldKey, ObjectHandle, ReadField, ReadFieldOf, WriteField, WriteFieldOf};

type Count = FieldKey<1>;
type Label = FieldKey<2>;

struct Record {
    count: Cell<u64>,
    label: String,
}

impl Field<Count> for Record {
    type Output = u64;
    type Storage = Self;
}

impl Field<Label> for Record {
    type Output = String;
    type Storage = Self;
}

impl ReadFieldOf<Self, Count, Infallible> for Record {
    fn read_field(owner: &Self, _: &Count) -> Result<u64, Infallible> {
        Ok(owner.count.get())
    }
}

impl WriteFieldOf<Self, Count, Infallible> for Record {
    fn write_field(owner: &Self, _: &Count, value: u64) -> Result<(), Infallible> {
        owner.count.set(value);
        Ok(())
    }
}

impl ReadFieldOf<ObjectHandle<Self>, Count, &'static str> for Record {
    fn read_field(owner: &ObjectHandle<Self>, _: &Count) -> Result<u64, &'static str> {
        owner.with(|state| if state.count.get() == 0 { Err("missing") } else { Ok(state.count.get()) })
    }
}

impl WriteFieldOf<ObjectHandle<Self>, Count, &'static str> for Record {
    fn write_field(owner: &ObjectHandle<Self>, _: &Count, value: u64) -> Result<(), &'static str> {
        if value == 0 {
            return Err("zero");
        }
        owner.with(|state| state.count.set(value));
        Ok(())
    }
}

impl ReadFieldOf<ObjectHandle<Self>, Label, Infallible> for Record {
    fn read_field(owner: &ObjectHandle<Self>, _: &Label) -> Result<String, Infallible> {
        Ok(owner.with(|state| state.label.clone()))
    }
}

fn update<Owner, Key, Error>(owner: &Owner, key: &Key, change: impl FnOnce(Owner::Output) -> Owner::Output) -> Result<(), Error>
where
    Owner: ReadField<Key, Error> + WriteField<Key, Error>,
{
    let value = <Owner as ReadField<Key, Error>>::read_field(owner, key)?;
    <Owner as WriteField<Key, Error>>::write_field(owner, key, change(value))
}

#[test]
fn dependent_fields_preserve_inline_storage_and_exact_integer_width() {
    assert_eq!(core::mem::size_of::<Count>(), 0);
    assert_eq!(core::mem::size_of::<Cell<u64>>(), core::mem::size_of::<u64>());
    let value = Record { count: Cell::new(9_007_199_254_740_993), label: "value".into() };
    update::<_, _, Infallible>(&value, &Count::default(), |count| count + 2).unwrap();
    assert_eq!(value.count.get(), 9_007_199_254_740_995);
    assert_eq!(value.label, "value");
}

#[test]
fn dependent_fields_preserve_shared_aliases_and_distinct_result_types() {
    let value = ObjectHandle::new(Record { count: Cell::new(7), label: "retained".into() });
    let alias = value.clone();
    update::<_, _, &'static str>(&value, &Count::default(), |count| count + 4).unwrap();
    assert_eq!(ReadField::<Count, &'static str>::read_field(&alias, &Count::default()), Ok(11));
    assert_eq!(ReadField::<Label, Infallible>::read_field(&alias, &Label::default()), Ok("retained".into()));
    assert!(ObjectHandle::same(&value, &alias));
}

#[test]
fn dependent_fields_propagate_read_and_write_failures_without_mutating() {
    let missing = ObjectHandle::new(Record { count: Cell::new(0), label: "missing".into() });
    assert_eq!(update::<_, _, &'static str>(&missing, &Count::default(), |_| panic!("read failed")), Err("missing"));
    let value = ObjectHandle::new(Record { count: Cell::new(7), label: "retained".into() });
    assert_eq!(WriteField::<Count, &'static str>::write_field(&value, &Count::default(), 0), Err("zero"));
    assert_eq!(value.with(|state| state.count.get()), 7);
}
