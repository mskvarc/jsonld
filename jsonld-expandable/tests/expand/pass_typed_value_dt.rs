use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Event")]
pub struct Event {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(
        property = "https://example.com/observedAt",
        typed_value,
        datatype = "http://www.w3.org/2001/XMLSchema#dateTime"
    )]
    pub observed_at: String,
}

fn main() {
    let e = Event {
        id: "urn:e:1".into(),
        observed_at: "2026-05-03T10:00:00Z".into(),
    };
    let v: serde_json::Value = e.expand();
    assert_eq!(
        v["https://example.com/observedAt"],
        serde_json::json!([
            {"@value": "2026-05-03T10:00:00Z", "@type": "http://www.w3.org/2001/XMLSchema#dateTime"}
        ])
    );
}
