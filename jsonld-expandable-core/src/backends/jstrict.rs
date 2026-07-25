//! `JsonValue` impl for [`jstrict::Value`].

use crate::JsonValue;
use jstrict::{Object, Value, object::Key};

impl JsonValue for Value {
    #[inline]
    fn null() -> Self {
        Value::Null
    }
    #[inline]
    fn bool(b: bool) -> Self {
        Value::Boolean(b)
    }
    #[inline]
    fn integer(n: i64) -> Self {
        Value::Number(n.into())
    }
    #[inline]
    fn unsigned(n: u64) -> Self {
        Value::Number(n.into())
    }
    #[inline]
    fn float(n: f64) -> Self {
        match jstrict::NumberBuf::try_from(n) {
            Ok(buf) => Value::Number(buf),
            Err(_) => Value::Null,
        }
    }
    #[inline]
    fn string(s: &str) -> Self {
        Value::String(jstrict::String::from(s))
    }
    #[inline]
    fn from_string(s: String) -> Self {
        Value::String(jstrict::String::from(s.as_str()))
    }
    #[inline]
    fn array<I: IntoIterator<Item = Self>>(items: I) -> Self {
        Value::Array(items.into_iter().collect())
    }
    #[inline]
    fn object<I: IntoIterator<Item = (String, Self)>>(entries: I) -> Self {
        let mut obj = Object::new();
        for (k, v) in entries {
            obj.push(Key::from(k.as_str()), v);
        }
        Value::Object(obj)
    }
    #[inline]
    fn into_object_entries(self) -> Option<Vec<(String, Self)>> {
        let obj = self.into_object()?;
        Some(
            obj.into_iter()
                .map(|entry| (entry.key.as_str().to_owned(), entry.value))
                .collect(),
        )
    }
}
