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
    #[inline]
    fn into_object_entries(self) -> Option<Vec<(String, Self)>> {
        let obj = self.into_object()?;
        Some(obj.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::JsonValue;
    use sonic_rs::{JsonValueTrait, Value};

    #[test]
    fn duplicate_keys_collapse_last_wins() {
        let v = Value::object([("a".to_string(), Value::integer(1)), ("a".to_string(), Value::integer(2))]);
        let entries = v.into_object_entries().expect("object");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "a");
        assert_eq!(entries[0].1.as_i64(), Some(2));
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
    fn object_entries_round_trip() {
        // Note: sonic-rs does not guarantee entry order, so assert by lookup.
        let v = Value::object([("b".to_string(), Value::bool(true)), ("a".to_string(), Value::unsigned(7))]);
        let entries = v.into_object_entries().expect("object");
        assert_eq!(entries.len(), 2);
        let a = entries.iter().find(|(k, _)| k == "a").expect("key a");
        assert_eq!(a.1.as_u64(), Some(7));
        let b = entries.iter().find(|(k, _)| k == "b").expect("key b");
        assert_eq!(b.1.as_bool(), Some(true));
    }
}
