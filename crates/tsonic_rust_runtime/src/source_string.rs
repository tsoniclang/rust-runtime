pub trait ToSourceString {
    fn to_source_string(&self) -> String;
}

pub fn source_string<T: ToSourceString + ?Sized>(value: &T) -> String {
    value.to_source_string()
}

macro_rules! impl_integer_source_string {
    ($($type:ty),+ $(,)?) => {
        $(impl ToSourceString for $type {
            fn to_source_string(&self) -> String {
                self.to_string()
            }
        })+
    };
}

impl_integer_source_string!(i8, u8, i16, u16, i32, u32, i64, u64, isize, usize);

impl ToSourceString for bool {
    fn to_source_string(&self) -> String {
        self.to_string()
    }
}

impl ToSourceString for str {
    fn to_source_string(&self) -> String {
        self.to_owned()
    }
}

impl ToSourceString for String {
    fn to_source_string(&self) -> String {
        self.clone()
    }
}

impl ToSourceString for () {
    fn to_source_string(&self) -> String {
        "undefined".to_owned()
    }
}

impl ToSourceString for crate::Undefined {
    fn to_source_string(&self) -> String {
        "undefined".to_owned()
    }
}

impl ToSourceString for crate::BigInt {
    fn to_source_string(&self) -> String {
        self.to_string()
    }
}

impl ToSourceString for f32 {
    fn to_source_string(&self) -> String {
        format_source_number(f64::from(*self))
    }
}

impl ToSourceString for f64 {
    fn to_source_string(&self) -> String {
        format_source_number(*self)
    }
}

fn format_source_number(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}
