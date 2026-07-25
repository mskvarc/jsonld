//! `ToJsonValue` impls for `chrono` types — gated on the `chrono` feature.
//!
//! Lets fields of foreign date/time types participate in expansion without
//! the consumer crate having to wrap them in a newtype to satisfy orphan
//! rules. All temporal types render as their xsd-style lexical form.

use crate::{JsonValue, ToJsonValue};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

impl<V: JsonValue, Tz: TimeZone> ToJsonValue<V> for DateTime<Tz>
where
    Tz::Offset: ::core::fmt::Display,
{
    #[inline]
    fn to_json_value(&self) -> V {
        V::from_string(self.to_rfc3339())
    }
}

impl<V: JsonValue> ToJsonValue<V> for NaiveDateTime {
    #[inline]
    fn to_json_value(&self) -> V {
        V::from_string(self.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
    }
}

impl<V: JsonValue> ToJsonValue<V> for NaiveDate {
    #[inline]
    fn to_json_value(&self) -> V {
        V::from_string(self.format("%Y-%m-%d").to_string())
    }
}

impl<V: JsonValue> ToJsonValue<V> for NaiveTime {
    #[inline]
    fn to_json_value(&self) -> V {
        V::from_string(self.format("%H:%M:%S%.f").to_string())
    }
}
