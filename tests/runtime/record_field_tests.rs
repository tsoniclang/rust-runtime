use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;
use tsonic_rust_runtime::{Location, ObjectHandle, ObjectIdentity, RecordField};

#[derive(Clone)]
struct Record {
    count: RecordField<i32, Infallible>,
    visits: usize,
}

#[test]
fn bound_field_copy_address_and_write_do_not_read_the_pointer() {
    let reads = Rc::new(Cell::new(0));
    let value = Rc::new(Cell::new(3));
    let reader_count = Rc::clone(&reads);
    let reader = Rc::clone(&value);
    let writer = Rc::clone(&value);
    let pointer = Location::<i32, Infallible>::try_bind(
        ObjectIdentity::new(),
        move || {
            reader_count.set(reader_count.get() + 1);
            Ok(reader.get())
        },
        move |next| {
            writer.set(next);
            Ok(())
        },
    );
    let owner = Location::<_, Infallible>::allocate(Record {
        count: RecordField::Bound(pointer.clone()),
        visits: 0,
    });
    let copied = owner.try_load().unwrap();
    let returned = Location::<_, Infallible>::allocate(copied);
    let address = RecordField::location_from_value(
        &returned,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    )
    .unwrap();
    assert!(Location::same(Some(&address), Some(&pointer)));
    assert_eq!(
        Location::hash(Some(&address)),
        Location::hash(Some(&pointer))
    );
    address.try_store(7).unwrap();
    assert_eq!(reads.get(), 0);
    assert_eq!(value.get(), 7);
    assert_eq!(owner.try_load().unwrap().count.try_load(), Ok(7));
    assert_eq!(reads.get(), 1);
}

#[test]
fn ordinary_record_copies_remain_independent_and_locations_remain_parent_relative() {
    let value = Record {
        count: RecordField::Value(3),
        visits: 0,
    };
    let mut copy = value.clone();
    copy.count.try_store(5).unwrap();
    assert_eq!(value.count.try_load(), Ok(3));
    let owner = Location::<_, Infallible>::allocate(value);
    let first = RecordField::location_from_value(
        &owner,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    )
    .unwrap();
    let alias = RecordField::location_from_value(
        &owner,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    )
    .unwrap();
    assert!(Location::same(Some(&first), Some(&alias)));
    owner
        .try_store(Record {
            count: RecordField::Value(11),
            visits: 0,
        })
        .unwrap();
    assert_eq!(first.try_load(), Ok(11));
    first.try_store(13).unwrap();
    assert_eq!(owner.try_load().unwrap().count.try_load(), Ok(13));
}

#[test]
fn reference_bound_callbacks_run_outside_the_record_borrow() {
    let owner = ObjectHandle::new(Record {
        count: RecordField::Value(0),
        visits: 0,
    });
    let reader = owner.clone();
    let writer = owner.clone();
    let pointer = Location::try_bind(
        ObjectIdentity::new(),
        move || {
            reader.with_mut(|record| record.visits += 1);
            Ok(19)
        },
        move |_| {
            writer.with_mut(|record| record.visits += 1);
            Ok(())
        },
    );
    owner.with_mut(|record| record.count = RecordField::Bound(pointer.clone()));
    let address = RecordField::location_from_object(
        &owner,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    );
    assert!(Location::same(Some(&address), Some(&pointer)));
    assert_eq!(owner.with(|record| record.visits), 0);
    assert_eq!(
        RecordField::try_load_object(&owner, |record| &record.count),
        Ok(19)
    );
    RecordField::try_store_object(
        &owner,
        |record| &record.count,
        |record| &mut record.count,
        23,
    )
    .unwrap();
    assert_eq!(owner.with(|record| record.visits), 2);
    owner.with_mut(|record| record.count = RecordField::Value(0));
}

#[test]
fn bound_fields_preserve_the_exact_error_and_retained_owner() {
    let owner = Rc::new(Cell::new(0));
    let retained = Rc::clone(&owner);
    let weak = Rc::downgrade(&owner);
    let failure = Rc::new(String::from("selected failure"));
    let read_failure = Rc::clone(&failure);
    let write_failure = Rc::clone(&failure);
    let pointer = Location::try_bind(
        ObjectIdentity::new(),
        move || {
            retained.set(retained.get() + 1);
            Err::<i32, _>(Rc::clone(&read_failure))
        },
        move |_| Err(Rc::clone(&write_failure)),
    );
    let mut field = RecordField::Bound(pointer);
    let copy = field.clone();
    drop(owner);
    assert!(weak.upgrade().is_some());
    assert!(Rc::ptr_eq(&copy.try_load().unwrap_err(), &failure));
    assert!(Rc::ptr_eq(&field.try_store(5).unwrap_err(), &failure));
    drop(field);
    assert!(weak.upgrade().is_some());
    drop(copy);
    assert!(weak.upgrade().is_none());
}

#[test]
fn replacing_a_bound_field_does_not_retarget_previously_taken_addresses() {
    let first = Location::<i32, Infallible>::allocate(3);
    let second = Location::<i32, Infallible>::allocate(5);
    let owner = ObjectHandle::new(Record {
        count: RecordField::Bound(first.clone()),
        visits: 0,
    });
    let old = RecordField::location_from_object(
        &owner,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    );
    owner.with_mut(|record| record.count = RecordField::Bound(second.clone()));
    let next = RecordField::location_from_object(
        &owner,
        "count".into(),
        |record| &record.count,
        |record| &mut record.count,
    );
    old.try_store(7).unwrap();
    next.try_store(11).unwrap();
    assert_eq!(first.try_load(), Ok(7));
    assert_eq!(second.try_load(), Ok(11));
    assert!(!Location::same(Some(&old), Some(&next)));
}
