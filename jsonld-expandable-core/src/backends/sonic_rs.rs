//! `JsonValue` impl for [`sonic_rs::Value`].

use crate::JsonValue;
use sonic_rs::{Object, Value};

impl JsonValue for Value {
    #[inline]
    fn null() -> Self {
        Value::default()
    }
    #[inline]
    fn bool(b: bool) -> Self {
        Value::from(b)
    }
    #[inline]
    fn integer(n: i64) -> Self {
        Value::from(n)
    }
    #[inline]
    fn unsigned(n: u64) -> Self {
        Value::from(n)
    }
    #[inline]
    fn float(n: f64) -> Self {
        Value::new_f64(n).unwrap_or_default()
    }
    #[inline]
    fn string(s: &str) -> Self {
        Value::from(s)
    }
    #[inline]
    fn from_string(s: String) -> Self {
        Value::from(s.as_str())
    }
    #[inline]
    fn array<I: IntoIterator<Item = Self>>(items: I) -> Self {
        items.into_iter().collect()
    }
    #[inline]
    fn object<I: IntoIterator<Item = (String, Self)>>(entries: I) -> Self {
        let mut obj = Object::new();
        for (k, v) in entries {
            obj.insert(&k, v);
        }
        Value::from(obj)
    }
}
