//! `JsonValue` impl for [`serde_json::Value`] plus a `ToJsonValue` impl
//! that lets a `serde_json::Value` field be projected into any backend.

use crate::{JsonValue, ToJsonValue};
use serde_json::{Map, Number, Value};

impl JsonValue for Value {
    #[inline]
    fn null() -> Self {
        Value::Null
    }
    #[inline]
    fn bool(b: bool) -> Self {
        Value::Bool(b)
    }
    #[inline]
    fn integer(n: i64) -> Self {
        Value::Number(Number::from(n))
    }
    #[inline]
    fn unsigned(n: u64) -> Self {
        Value::Number(Number::from(n))
    }
    #[inline]
    fn float(n: f64) -> Self {
        Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
    #[inline]
    fn string(s: &str) -> Self {
        Value::String(s.to_owned())
    }
    #[inline]
    fn from_string(s: String) -> Self {
        Value::String(s)
    }
    #[inline]
    fn array<I: IntoIterator<Item = Self>>(items: I) -> Self {
        Value::Array(items.into_iter().collect())
    }
    #[inline]
    fn object<I: IntoIterator<Item = (String, Self)>>(entries: I) -> Self {
        let mut map = Map::new();
        for (k, v) in entries {
            map.insert(k, v);
        }
        Value::Object(map)
    }
    #[inline]
    fn into_object_entries(self) -> Option<Vec<(String, Self)>> {
        match self {
            Value::Object(map) => Some(map.into_iter().collect()),
            _ => None,
        }
    }
}

/// Recursively translate a [`serde_json::Value`] into the chosen `JsonValue`
/// backend. Lets fields whose type is `serde_json::Value` participate in
/// generic expansion without forcing the backend to *be* `serde_json`.
impl<V: JsonValue> ToJsonValue<V> for Value {
    fn to_json_value(&self) -> V {
        match self {
            Value::Null => V::null(),
            Value::Bool(b) => V::bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    V::integer(i)
                } else if let Some(u) = n.as_u64() {
                    V::unsigned(u)
                } else if let Some(f) = n.as_f64() {
                    V::float(f)
                } else {
                    V::null()
                }
            }
            Value::String(s) => V::string(s),
            Value::Array(arr) => V::array(arr.iter().map(ToJsonValue::<V>::to_json_value)),
            Value::Object(map) => {
                V::object(map.iter().map(|(k, v)| (k.clone(), v.to_json_value())))
            }
        }
    }
}
