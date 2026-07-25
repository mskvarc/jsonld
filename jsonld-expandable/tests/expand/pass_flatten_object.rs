use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(fragment)]
pub struct System {
    #[jsonld(property = "https://example.com/createdAt")]
    pub created_at: String,
}

#[derive(Expandable)]
#[jsonld(fragment)]
pub struct Common {
    #[jsonld(property = "https://example.com/datasetId", coerce = "@id")]
    pub dataset_id: String,
    #[jsonld(flatten_object)]
    pub system: System,
}

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Property")]
pub struct Property {
    #[jsonld(property = "https://example.com/value")]
    pub value: String,
    #[jsonld(flatten_object)]
    pub common: Common,
    #[jsonld(flatten_object)]
    pub maybe_extra: Option<System>,
}

fn main() {
    let p = Property {
        value: "x".to_string(),
        common: Common {
            dataset_id: "urn:ds:1".to_string(),
            system: System {
                created_at: "2026-05-03T10:00:00Z".to_string(),
            },
        },
        maybe_extra: Some(System {
            created_at: "2026-05-03T11:00:00Z".to_string(),
        }),
    };
    let v: serde_json::Value = p.expand();
    let obj = v.as_object().unwrap();
    assert_eq!(obj["@type"], serde_json::json!(["https://example.com/Property"]));
    assert_eq!(obj["https://example.com/value"], serde_json::json!([{"@value": "x"}]));
    // Merged from Common (flatten):
    assert_eq!(
        obj["https://example.com/datasetId"],
        serde_json::json!([{"@id": "urn:ds:1"}])
    );
    // Merged transitively via Common -> System (nested flatten), then
    // overwritten by Option<System> (last-write-wins in serde_json::Map):
    assert_eq!(
        obj["https://example.com/createdAt"],
        serde_json::json!([{"@value": "2026-05-03T11:00:00Z"}])
    );
}
