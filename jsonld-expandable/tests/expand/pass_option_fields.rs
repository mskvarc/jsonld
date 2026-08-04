use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct Thing {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/count")]
    pub count: Option<i64>,

    #[jsonld(property = "https://example.com/names", coerce = "@id", vec)]
    pub names: Option<Vec<String>>,

    #[jsonld(property = "https://example.com/category", coerce = "@id")]
    pub category: Option<String>,

    #[jsonld(property = "https://example.com/tags", coerce = "@id", vec)]
    pub tags: Vec<String>,
}

fn main() {
    let thing = Thing {
        id: "urn:thing:1".to_string(),
        count: Some(42),
        names: Some(vec!["alpha".to_string(), "beta".to_string()]),
        category: Some("https://example.com/Cat".to_string()),
        tags: vec!["https://example.com/A".to_string()],
    };
    let v: serde_json::Value = thing.expand();
    let obj = v.as_object().unwrap();
    assert_eq!(obj["https://example.com/count"], serde_json::json!([{"@value": 42}]));
    assert_eq!(
        obj["https://example.com/names"],
        serde_json::json!([{"@id": "alpha"}, {"@id": "beta"}])
    );
    assert_eq!(
        obj["https://example.com/category"],
        serde_json::json!([{"@id": "https://example.com/Cat"}])
    );
    assert_eq!(
        obj["https://example.com/tags"],
        serde_json::json!([{"@id": "https://example.com/A"}])
    );

    let none_thing = Thing {
        id: "urn:thing:2".into(),
        count: None,
        names: None,
        category: None,
        tags: vec![],
    };
    let v: serde_json::Value = none_thing.expand();
    let obj = v.as_object().unwrap();
    assert!(obj.get("https://example.com/count").is_none());
    assert!(obj.get("https://example.com/names").is_none());
    assert!(obj.get("https://example.com/category").is_none());
    assert!(obj.get("https://example.com/tags").is_some());
}
