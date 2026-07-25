use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;
use std::collections::BTreeMap;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Sub")]
pub struct Sub {
    #[jsonld(property = "https://example.com/v")]
    pub v: i64,
}

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Parent")]
pub struct Parent {
    #[jsonld(property = "https://example.com/name")]
    pub name: String,
    #[jsonld(flatten_map)]
    pub subs: BTreeMap<String, Sub>,
}

fn main() {
    let mut subs = BTreeMap::new();
    subs.insert("https://example.com/a".to_string(), Sub { v: 1 });
    subs.insert("https://example.com/b".to_string(), Sub { v: 2 });
    let p = Parent {
        name: "n".to_string(),
        subs,
    };
    let v: serde_json::Value = p.expand();
    let obj = v.as_object().unwrap();
    assert_eq!(obj["@type"], serde_json::json!(["https://example.com/Parent"]));
    assert_eq!(obj["https://example.com/name"], serde_json::json!([{"@value": "n"}]));
    // Each map entry's key is now a top-level property pointing at the
    // recursively expanded Sub object (single object, not wrapped in an array
    // — matches andromeda's legacy behavior).
    let a = &obj["https://example.com/a"];
    assert_eq!(a["@type"], serde_json::json!(["https://example.com/Sub"]));
    assert_eq!(a["https://example.com/v"], serde_json::json!([{"@value": 1}]));
    let b = &obj["https://example.com/b"];
    assert_eq!(b["https://example.com/v"], serde_json::json!([{"@value": 2}]));
}
