//! `JsonValue` impl for [`serde_json::Value`].

use crate::JsonValue;
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
}
