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
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == f64::INFINITY {
        return "Infinity".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }

    let negative = value.is_sign_negative();
    let text = value.abs().to_string();
    let (mantissa, explicit_exponent) = text
        .split_once(['e', 'E'])
        .map_or((text.as_str(), 0), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().unwrap_or(0))
        });
    let decimal_position = mantissa.find('.').unwrap_or(mantissa.len());
    let raw_digits = mantissa
        .bytes()
        .filter(|byte| *byte != b'.')
        .map(char::from)
        .collect::<String>();
    let leading_zeroes = raw_digits.bytes().take_while(|byte| *byte == b'0').count();
    let mut digits = raw_digits[leading_zeroes..].to_owned();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let decimal_exponent = i32::try_from(decimal_position)
        .unwrap_or(i32::MAX)
        .saturating_sub(i32::try_from(leading_zeroes).unwrap_or(i32::MAX))
        .saturating_add(explicit_exponent);
    let digit_count = i32::try_from(digits.len()).unwrap_or(i32::MAX);

    let unsigned = if digit_count <= decimal_exponent && decimal_exponent <= 21 {
        format!(
            "{digits}{}",
            "0".repeat(usize::try_from(decimal_exponent - digit_count).unwrap_or(0))
        )
    } else if 0 < decimal_exponent && decimal_exponent <= 21 {
        let split = usize::try_from(decimal_exponent).unwrap_or(digits.len());
        format!("{}.{}", &digits[..split], &digits[split..])
    } else if -6 < decimal_exponent && decimal_exponent <= 0 {
        format!(
            "0.{}{digits}",
            "0".repeat(usize::try_from(-decimal_exponent).unwrap_or(0))
        )
    } else {
        let mut scientific = digits[..1].to_owned();
        if digits.len() > 1 {
            scientific.push('.');
            scientific.push_str(&digits[1..]);
        }
        let exponent = decimal_exponent - 1;
        scientific.push('e');
        if exponent >= 0 {
            scientific.push('+');
        }
        scientific.push_str(&exponent.to_string());
        scientific
    };

    if negative {
        format!("-{unsigned}")
    } else {
        unsigned
    }
}
