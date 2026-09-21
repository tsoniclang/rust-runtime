#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FieldKey<const ID: u128>;

pub trait Field<Key> {
    type Output;
    type Storage;
}

pub trait ReadField<Key, Error>: Field<Key> {
    fn read_field(&self, key: &Key) -> Result<Self::Output, Error>;
}

pub trait WriteField<Key, Error>: Field<Key> {
    fn write_field(&mut self, key: &Key, value: Self::Output) -> Result<(), Error>;
}

pub trait ReadFieldOf<Owner: Field<Key>, Key, Error> {
    fn read_field(owner: &Owner, key: &Key) -> Result<Owner::Output, Error>;
}

pub trait WriteFieldOf<Owner: Field<Key>, Key, Error> {
    fn write_field(owner: &mut Owner, key: &Key, value: Owner::Output) -> Result<(), Error>;
}

impl<Owner, Key, Error> ReadField<Key, Error> for Owner
where
    Owner: Field<Key>,
    Owner::Storage: ReadFieldOf<Owner, Key, Error>,
{
    fn read_field(&self, key: &Key) -> Result<Self::Output, Error> {
        Owner::Storage::read_field(self, key)
    }
}

impl<Owner, Key, Error> WriteField<Key, Error> for Owner
where
    Owner: Field<Key>,
    Owner::Storage: WriteFieldOf<Owner, Key, Error>,
{
    fn write_field(&mut self, key: &Key, value: Self::Output) -> Result<(), Error> {
        Owner::Storage::write_field(self, key, value)
    }
}

#[cfg(feature = "alloc")]
impl<Storage: Field<Key>, Key> Field<Key> for crate::ObjectHandle<Storage> {
    type Output = Storage::Output;
    type Storage = Storage::Storage;
}

#[cfg(feature = "alloc")]
impl<Storage: Field<Key>, Key> Field<Key> for crate::ObjectRef<Storage> {
    type Output = Storage::Output;
    type Storage = Storage::Storage;
}
