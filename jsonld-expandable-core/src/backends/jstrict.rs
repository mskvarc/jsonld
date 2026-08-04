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
            // `insert`, not `push`: duplicate keys must collapse (last wins)
            // exactly like the serde_json and sonic-rs backends, so `flatten`
            // collisions produce the same document on every backend.
            obj.insert(Key::from(k.as_str()), v);
        }
        Value::Object(obj)
    }
    #[inline]
    fn into_object_entries(self) -> Option<Vec<(String, Self)>> {
        let obj = self.into_object()?;
        Some(obj.into_iter().map(|entry| (entry.key.as_str().to_owned(), entry.value)).collect())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::JsonValue;
    use jstrict::Value;

    #[test]
    fn duplicate_keys_collapse_last_wins() {
        let v = Value::object([("a".to_string(), Value::integer(1)), ("a".to_string(), Value::integer(2))]);
        let entries = v.into_object_entries().expect("object");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "a");
        assert_eq!(entries[0].1, Value::integer(2));
    }

    #[test]
    fn non_finite_floats_become_null() {
        assert!(Value::float(f64::NAN).is_null());
        assert!(Value::float(f64::INFINITY).is_null());
        assert!(!Value::float(1.5).is_null());
    }

    #[test]
    fn into_object_entries_is_none_for_non_objects() {
        assert!(Value::string("x").into_object_entries().is_none());
        assert!(Value::array([Value::null()]).into_object_entries().is_none());
    }

    #[test]
    fn object_entries_round_trip_in_order() {
        let v = Value::object([("b".to_string(), Value::bool(true)), ("a".to_string(), Value::unsigned(7))]);
        let entries = v.into_object_entries().expect("object");
        assert_eq!(entries, vec![("b".to_string(), Value::bool(true)), ("a".to_string(), Value::unsigned(7))]);
    }
}
