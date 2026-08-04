//! Backend-agnostic JSON value abstraction.

use std::collections::{BTreeMap, HashMap};

/// Minimal vocabulary for any JSON value type the derive can construct.
///
/// Implement this once per backend value type. The derive's generated code
/// never names a concrete value type; consumers select one at the call site.
///
/// ```ignore
/// let v: serde_json::Value = thing.expand();      // inferred
/// let v = thing.expand::<jstrict::Value>();        // turbofish
/// ```
pub trait JsonValue: Sized {
    /// Builds the null value.
    fn null() -> Self;
    /// Builds a boolean value.
    fn bool(b: bool) -> Self;
    /// Builds a signed integer value.
    fn integer(n: i64) -> Self;
    /// Builds an unsigned integer value.
    fn unsigned(n: u64) -> Self;
    /// Non-finite (`NaN`, `±Infinity`) values become [`Self::null`] — JSON has
    /// no representation for them.
    fn float(n: f64) -> Self;
    /// Builds a string value from a slice.
    fn string(s: &str) -> Self;
    /// Builds a string value from an owned string.
    #[must_use]
    fn from_string(s: String) -> Self {
        Self::string(&s)
    }
    /// Builds an array from the given items.
    fn array<I: IntoIterator<Item = Self>>(items: I) -> Self;
    /// Builds an object from the given entries.
    fn object<I: IntoIterator<Item = (String, Self)>>(entries: I) -> Self;
    /// Destructure an object value back into its entries. Returns `None` for
    /// non-object variants. Used by `flatten` to merge a sub-fragment into its
    /// parent.
    fn into_object_entries(self) -> Option<Vec<(String, Self)>>;
}

/// Convert a leaf field value to the chosen `JsonValue` backend.
///
/// Generated code calls this for fields that should land in a `{"@value": ...}`
/// object (i.e. anything not marked `nested` / `coerce` / `container`).
pub trait ToJsonValue<V: JsonValue> {
    /// Converts this value into the chosen JSON backend `V`.
    fn to_json_value(&self) -> V;
}

macro_rules! impl_int {
    ($($t:ty)*) => {$(
        impl<V: JsonValue> ToJsonValue<V> for $t {
            #[inline]
            fn to_json_value(&self) -> V { V::integer(i64::from(*self)) }
        }
    )*};
}
impl_int!(i8 i16 i32 i64);

macro_rules! impl_uint_fits {
    ($($t:ty)*) => {$(
        impl<V: JsonValue> ToJsonValue<V> for $t {
            #[inline]
            fn to_json_value(&self) -> V { V::unsigned(u64::from(*self)) }
        }
    )*};
}
impl_uint_fits!(u8 u16 u32 u64);

impl<V: JsonValue> ToJsonValue<V> for usize {
    #[inline]
    fn to_json_value(&self) -> V {
        V::unsigned(*self as u64)
    }
}

impl<V: JsonValue> ToJsonValue<V> for isize {
    #[inline]
    fn to_json_value(&self) -> V {
        V::integer(*self as i64)
    }
}

impl<V: JsonValue> ToJsonValue<V> for f32 {
    #[inline]
    fn to_json_value(&self) -> V {
        V::float(f64::from(*self))
    }
}

impl<V: JsonValue> ToJsonValue<V> for f64 {
    #[inline]
    fn to_json_value(&self) -> V {
        V::float(*self)
    }
}

impl<V: JsonValue> ToJsonValue<V> for bool {
    #[inline]
    fn to_json_value(&self) -> V {
        V::bool(*self)
    }
}

impl<V: JsonValue> ToJsonValue<V> for str {
    #[inline]
    fn to_json_value(&self) -> V {
        V::string(self)
    }
}

impl<V: JsonValue> ToJsonValue<V> for String {
    #[inline]
    fn to_json_value(&self) -> V {
        V::string(self.as_str())
    }
}

impl<V: JsonValue, T: ToJsonValue<V> + ?Sized> ToJsonValue<V> for &T {
    #[inline]
    fn to_json_value(&self) -> V {
        (*self).to_json_value()
    }
}

impl<V: JsonValue, T: ToJsonValue<V>> ToJsonValue<V> for Vec<T> {
    #[inline]
    fn to_json_value(&self) -> V {
        V::array(self.iter().map(ToJsonValue::to_json_value))
    }
}

impl<V: JsonValue, T: ToJsonValue<V>> ToJsonValue<V> for [T] {
    #[inline]
    fn to_json_value(&self) -> V {
        V::array(self.iter().map(ToJsonValue::to_json_value))
    }
}

impl<V: JsonValue, T: ToJsonValue<V>> ToJsonValue<V> for Option<T> {
    #[inline]
    fn to_json_value(&self) -> V {
        match self {
            Some(t) => t.to_json_value(),
            None => V::null(),
        }
    }
}

impl<V: JsonValue, K, T, S> ToJsonValue<V> for HashMap<K, T, S>
where
    K: AsRef<str>,
    T: ToJsonValue<V>,
{
    #[inline]
    fn to_json_value(&self) -> V {
        V::object(self.iter().map(|(k, v)| (k.as_ref().to_string(), v.to_json_value())))
    }
}

impl<V: JsonValue, K, T> ToJsonValue<V> for BTreeMap<K, T>
where
    K: AsRef<str>,
    T: ToJsonValue<V>,
{
    #[inline]
    fn to_json_value(&self) -> V {
        V::object(self.iter().map(|(k, v)| (k.as_ref().to_string(), v.to_json_value())))
    }
}
