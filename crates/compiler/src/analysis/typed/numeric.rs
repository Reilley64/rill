//! Scalar conversion operations and the independent protected numeric matrix.
use super::{Primitive, PrimitiveValue};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Width {
    Integer { bits: u32, signed: bool },
    Float,
}

fn width(primitive: Primitive) -> Option<Width> {
    use Primitive::*;
    let (bits, signed) = match primitive {
        I8 => (8, true),
        I16 => (16, true),
        I32 => (32, true),
        I64 => (64, true),
        I128 => (128, true),
        U8 => (8, false),
        U16 => (16, false),
        U32 => (32, false),
        U64 => (64, false),
        U128 => (128, false),
        F32 | F64 => return Some(Width::Float),
        Bool | Char | String => return None,
    };
    Some(Width::Integer { bits, signed })
}

/// The protected infallible policy. A gap prohibits an application registration;
/// it is not inferred from missing providers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfallibleNumericContract {
    RequiredProvider,
    Gap,
}

impl InfallibleNumericContract {
    pub fn for_pair(destination: Primitive, source: Primitive) -> Option<Self> {
        use Width::*;
        let allowed = match (width(destination)?, width(source)?) {
            (Float, _) => true,
            (Integer { .. }, Float) => false,
            (
                Integer {
                    bits: to,
                    signed: to_signed,
                },
                Integer {
                    bits: from,
                    signed: from_signed,
                },
            ) => {
                if to_signed == from_signed {
                    to >= from
                } else {
                    to_signed && to > from
                }
            }
        };
        Some(if allowed {
            Self::RequiredProvider
        } else {
            Self::Gap
        })
    }
}

/// The implemented scalar operation. The Rust result here is not a Rill Result or Err.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumericConversion {
    destination: Primitive,
    source: Primitive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericConversionError {
    OutOfRange,
    NotFinite,
}

impl NumericConversion {
    pub fn for_pair(destination: Primitive, source: Primitive) -> Option<Self> {
        // This table describes operation implementations, not availability of into.
        let numeric = |p| !matches!(p, Primitive::Bool | Primitive::Char | Primitive::String);
        (numeric(destination) && numeric(source)).then_some(Self {
            destination,
            source,
        })
    }

    pub fn destination(self) -> Primitive {
        self.destination
    }
    pub fn source(self) -> Primitive {
        self.source
    }

    pub fn convert(
        self,
        value: PrimitiveValue<'_>,
    ) -> Option<Result<PrimitiveValue<'static>, NumericConversionError>> {
        if value.primitive() != self.source {
            return None;
        }
        use Primitive::*;
        Some(match self.destination {
            I8 => to_integer!(value, I8, i8),
            I16 => to_integer!(value, I16, i16),
            I32 => to_integer!(value, I32, i32),
            I64 => to_integer!(value, I64, i64),
            I128 => to_integer!(value, I128, i128),
            U8 => to_integer!(value, U8, u8),
            U16 => to_integer!(value, U16, u16),
            U32 => to_integer!(value, U32, u32),
            U64 => to_integer!(value, U64, u64),
            U128 => to_integer!(value, U128, u128),
            F32 => Ok(PrimitiveValue::F32(to_float!(value, f32))),
            F64 => Ok(PrimitiveValue::F64(to_float!(value, f64))),
            Bool | Char | String => unreachable!("numeric operation has a numeric destination"),
        })
    }
}

// The rounding of an integer to f32 must not pass through f64 first.
macro_rules! to_float {
    ($value:expr, $dest:ty) => {{
        use PrimitiveValue::*;
        match $value {
            I8(v) => v as $dest,
            I16(v) => v as $dest,
            I32(v) => v as $dest,
            I64(v) => v as $dest,
            I128(v) => v as $dest,
            U8(v) => v as $dest,
            U16(v) => v as $dest,
            U32(v) => v as $dest,
            U64(v) => v as $dest,
            U128(v) => v as $dest,
            F32(v) => v as $dest,
            F64(v) => v as $dest,
            Bool(_) | Char(_) | String(_) => unreachable!("numeric input"),
        }
    }};
}

macro_rules! to_integer {
    ($value:expr, $variant:ident, $dest:ty) => {{
        use PrimitiveValue::*;
        let checked = match $value {
            I8(v) => <$dest>::try_from(v).ok(),
            I16(v) => <$dest>::try_from(v).ok(),
            I32(v) => <$dest>::try_from(v).ok(),
            I64(v) => <$dest>::try_from(v).ok(),
            I128(v) => <$dest>::try_from(v).ok(),
            U8(v) => <$dest>::try_from(v).ok(),
            U16(v) => <$dest>::try_from(v).ok(),
            U32(v) => <$dest>::try_from(v).ok(),
            U64(v) => <$dest>::try_from(v).ok(),
            U128(v) => <$dest>::try_from(v).ok(),
            F32(v) => {
                return Some(
                    float_integer(f64::from(v), <$dest>::BITS, <$dest>::MIN != 0)
                        .map(|v| $variant(v as $dest)),
                );
            }
            F64(v) => {
                return Some(
                    float_integer(v, <$dest>::BITS, <$dest>::MIN != 0)
                        .map(|v| $variant(v as $dest)),
                );
            }
            Bool(_) | Char(_) | String(_) => unreachable!("numeric input"),
        };
        checked
            .map($variant)
            .ok_or(NumericConversionError::OutOfRange)
    }};
}

use {to_float, to_integer};

fn float_integer(value: f64, bits: u32, signed: bool) -> Result<f64, NumericConversionError> {
    if !value.is_finite() {
        return Err(NumericConversionError::NotFinite);
    }
    let truncated = value.trunc();
    // Powers of two are exact, including 2^128. Max integer endpoints are not.
    let upper = 2.0f64.powi((bits - u32::from(signed)) as i32);
    let lower = if signed { -upper } else { 0.0 };
    if truncated < lower || truncated >= upper {
        return Err(NumericConversionError::OutOfRange);
    }
    Ok(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PrimitiveValue as V;

    #[test]
    fn each_numeric_pair_has_an_operation_but_only_64_have_infallible_contracts() {
        use Primitive::*;
        let examples = [
            (V::I8(i8::MIN), 7),
            (V::I16(i16::MIN), 6),
            (V::I32(i32::MIN), 5),
            (V::I64(i64::MIN), 4),
            (V::I128(i128::MIN), 3),
            (V::U8(u8::MAX), 11),
            (V::U16(u16::MAX), 9),
            (V::U32(u32::MAX), 7),
            (V::U64(u64::MAX), 5),
            (V::U128(u128::MAX), 3),
            (V::F32(f32::MAX), 2),
            (V::F64(f64::MAX), 2),
        ];
        for (value, allowed) in examples {
            let from = value.primitive();
            let mut required = 0;
            for to in [I8, I16, I32, I64, I128, U8, U16, U32, U64, U128, F32, F64] {
                let contract = InfallibleNumericContract::for_pair(to, from).unwrap();
                let operation = NumericConversion::for_pair(to, from).unwrap();
                let result = operation.convert(value).unwrap();
                if contract == InfallibleNumericContract::RequiredProvider {
                    required += 1;
                    assert_eq!(result.unwrap().primitive(), to);
                }
            }
            assert_eq!(required, allowed, "from {from:?}");
        }
        assert!(InfallibleNumericContract::for_pair(I32, Bool).is_none());
        assert!(NumericConversion::for_pair(I32, Bool).is_none());
    }

    #[test]
    fn numeric_operation_checks_range_after_truncation_without_lossy_boundary_casts() {
        let convert = |to, from: V<'_>| {
            NumericConversion::for_pair(to, from.primitive())
                .unwrap()
                .convert(from)
                .unwrap()
        };
        use NumericConversionError::*;
        assert_eq!(convert(Primitive::U8, V::F64(-0.5)), Ok(V::U8(0)));
        assert_eq!(convert(Primitive::U8, V::F64(-1.0)), Err(OutOfRange));
        assert_eq!(convert(Primitive::U8, V::F64(f64::NAN)), Err(NotFinite));
        assert_eq!(
            convert(Primitive::U128, V::F64(2.0f64.powi(128))),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::I128, V::F64(-2.0f64.powi(127))),
            Ok(V::I128(i128::MIN))
        );
        assert_eq!(
            convert(Primitive::I128, V::F64(2.0f64.powi(127))),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::U128, V::U128(u128::MAX)),
            Ok(V::U128(u128::MAX))
        );
        assert_eq!(
            convert(Primitive::I128, V::U128(u128::MAX)),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::F32, V::U128(u128::MAX)),
            Ok(V::F32(f32::INFINITY))
        );
    }
}
