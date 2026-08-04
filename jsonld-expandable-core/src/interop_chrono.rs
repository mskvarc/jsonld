//! `ToJsonValue` impls for `chrono` types — gated on the `chrono` feature.
//!
//! Lets fields of foreign date/time types participate in expansion without
//! the consumer crate having to wrap them in a newtype to satisfy orphan
//! rules. All temporal types render as their xsd-style lexical form.

use crate::{JsonValue, ToJsonValue};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeZone};

impl<V: JsonValue, Tz: TimeZone> ToJsonValue<V> for DateTime<Tz>
where
    Tz::Offset: ::core::fmt::Display,
{
    #[inline]
    fn to_json_value(&self) -> V {
        // `use_z: true` renders UTC as the canonical `Z` suffix instead of
        // `+00:00`; NGSI-LD brokers expect the former.
        V::from_string(self.to_rfc3339_opts(SecondsFormat::AutoSi, true))
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

#[cfg(all(test, feature = "serde-json"))]
#[allow(clippy::panic)]
mod tests {
    use crate::ToJsonValue;
    use chrono::{TimeZone, Utc};

    #[test]
    fn utc_datetimes_render_with_z_suffix() {
        let Some(dt) = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).single() else {
            panic!("valid timestamp");
        };
        let v: serde_json::Value = dt.to_json_value();
        assert_eq!(v, serde_json::json!("2026-01-02T03:04:05Z"));
    }
}
