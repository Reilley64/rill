//! Implemented scalar operations. These values are not an ABI or a Rill storage layout.
macro_rules! primitives {
    ($($variant:ident($type:ty) => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum Primitive { $($variant),+ }

        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum PrimitiveValue<'a> { $($variant($type)),+ }

        impl Primitive {
            pub(in crate::analysis) fn named(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl PrimitiveValue<'_> {
            pub fn primitive(self) -> Primitive {
                match self { $(Self::$variant(_) => Primitive::$variant),+ }
            }
        }
    };
}

primitives! {
    Bool(bool) => "bool", Char(char) => "char", String(&'a str) => "string",
    I8(i8) => "i8", I16(i16) => "i16", I32(i32) => "i32", I64(i64) => "i64", I128(i128) => "i128",
    U8(u8) => "u8", U16(u16) => "u16", U32(u32) => "u32", U64(u64) => "u64", U128(u128) => "u128",
    F32(f32) => "f32", F64(f64) => "f64",
}

impl Primitive {
    /// Default rendering of a scalar at the width selected by the checker.
    /// Returns None for a value that does not match the selected operation.
    pub fn format(self, value: PrimitiveValue<'_>) -> Option<String> {
        if self != value.primitive() {
            return None;
        }
        use PrimitiveValue::*;
        Some(match value {
            Bool(v) => v.to_string(),
            Char(v) => v.to_string(),
            String(v) => v.to_owned(),
            I8(v) => v.to_string(),
            I16(v) => v.to_string(),
            I32(v) => v.to_string(),
            I64(v) => v.to_string(),
            I128(v) => v.to_string(),
            U8(v) => v.to_string(),
            U16(v) => v.to_string(),
            U32(v) => v.to_string(),
            U64(v) => v.to_string(),
            U128(v) => v.to_string(),
            F32(v) => format_float(v, v.is_finite()),
            F64(v) => format_float(v, v.is_finite()),
        })
    }
}

fn format_float(value: impl std::fmt::Display + std::fmt::LowerExp, finite: bool) -> String {
    let decimal = value.to_string();
    if !finite {
        return decimal;
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("Rust LowerExp output");
    let integral = mantissa.split_once('.').map(|(whole, fraction)| {
        // Keeping the same shortest significant digits, removing the decimal
        // point can shorten the spelling (1.2e100 -> 12e99).
        let exponent: i32 = exponent.parse().expect("Rust LowerExp exponent");
        let exponent = exponent - fraction.len() as i32;
        format!("{whole}{fraction}e{exponent}")
    });
    [Some(decimal), Some(scientific), integral]
        .into_iter()
        .flatten()
        .min_by_key(String::len)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::{Primitive, PrimitiveValue as Value};

    #[test]
    fn primitive_show_operations_produce_owned_default_text() {
        let original = String::from("café");
        let text = Primitive::String.format(Value::String(&original)).unwrap();
        drop(original);
        assert_eq!(text, "café");
        for (input, expected) in [
            (Value::Bool(false), "false"),
            (Value::Char('🦀'), "🦀"),
            (
                Value::I128(i128::MIN),
                "-170141183460469231731687303715884105728",
            ),
            (Value::F32(-0.0), "-0"),
            (Value::F64(f64::NEG_INFINITY), "-inf"),
            (Value::F32(f32::NAN), "NaN"),
            (Value::F32(0.5), "0.5"),
            (Value::F64(1e100), "1e100"),
            (Value::F64(1.2e100), "12e99"),
        ] {
            assert_eq!(input.primitive().format(input).as_deref(), Some(expected));
        }
        assert_eq!(Primitive::U32.format(Value::I32(0)), None);
        for value in [
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::MAX,
            1.234_567,
            -0.0,
        ] {
            let text = Primitive::F32.format(Value::F32(value)).unwrap();
            assert_eq!(text.parse::<f32>().unwrap().to_bits(), value.to_bits());
        }
    }
}
