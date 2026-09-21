use core::convert::Infallible;
use tsonic_rust_runtime::{Field, FieldKey, ObjectHandle, ReadField, ReadFieldOf, WriteField, WriteFieldOf};

type Count = FieldKey<1>;
type Label = FieldKey<2>;

struct Record {
    count: u64,
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
        Ok(owner.count)
    }
}

impl WriteFieldOf<Self, Count, Infallible> for Record {
    fn write_field(owner: &mut Self, _: &Count, value: u64) -> Result<(), Infallible> {
        owner.count = value;
        Ok(())
    }
}

impl ReadFieldOf<ObjectHandle<Self>, Count, &'static str> for Record {
    fn read_field(owner: &ObjectHandle<Self>, _: &Count) -> Result<u64, &'static str> {
        owner.with(|state| if state.count == 0 { Err("missing") } else { Ok(state.count) })
    }
}

impl WriteFieldOf<ObjectHandle<Self>, Count, &'static str> for Record {
    fn write_field(owner: &mut ObjectHandle<Self>, _: &Count, value: u64) -> Result<(), &'static str> {
        if value == 0 {
            return Err("zero");
        }
        owner.with_mut(|state| state.count = value);
        Ok(())
    }
}

impl ReadFieldOf<ObjectHandle<Self>, Label, Infallible> for Record {
    fn read_field(owner: &ObjectHandle<Self>, _: &Label) -> Result<String, Infallible> {
        Ok(owner.with(|state| state.label.clone()))
    }
}

fn update<Owner, Key, Error>(owner: &mut Owner, key: &Key, change: impl FnOnce(Owner::Output) -> Owner::Output) -> Result<(), Error>
where
    Owner: ReadField<Key, Error> + WriteField<Key, Error>,
{
    let value = <Owner as ReadField<Key, Error>>::read_field(owner, key)?;
    <Owner as WriteField<Key, Error>>::write_field(owner, key, change(value))
}

#[test]
fn dependent_fields_preserve_inline_storage_and_exact_integer_width() {
    assert_eq!(core::mem::size_of::<Count>(), 0);
    let mut value = Record { count: 9_007_199_254_740_993, label: "value".into() };
    update::<_, _, Infallible>(&mut value, &Count::default(), |count| count + 2).unwrap();
    assert_eq!(value.count, 9_007_199_254_740_995);
    assert_eq!(value.label, "value");
}

#[test]
fn dependent_fields_preserve_shared_aliases_and_distinct_result_types() {
    let mut value = ObjectHandle::new(Record { count: 7, label: "retained".into() });
    let alias = value.clone();
    update::<_, _, &'static str>(&mut value, &Count::default(), |count| count + 4).unwrap();
    assert_eq!(ReadField::<Count, &'static str>::read_field(&alias, &Count::default()), Ok(11));
    assert_eq!(ReadField::<Label, Infallible>::read_field(&alias, &Label::default()), Ok("retained".into()));
    assert!(ObjectHandle::same(&value, &alias));
}

#[test]
fn dependent_fields_propagate_read_and_write_failures_without_mutating() {
    let mut missing = ObjectHandle::new(Record { count: 0, label: "missing".into() });
    assert_eq!(update::<_, _, &'static str>(&mut missing, &Count::default(), |_| panic!("read failed")), Err("missing"));
    let mut value = ObjectHandle::new(Record { count: 7, label: "retained".into() });
    assert_eq!(WriteField::<Count, &'static str>::write_field(&mut value, &Count::default(), 0), Err("zero"));
    assert_eq!(value.with(|state| state.count), 7);
}
