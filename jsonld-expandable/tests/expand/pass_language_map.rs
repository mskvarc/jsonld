use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;
use std::collections::{BTreeMap, HashMap};

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Labelled")]
pub struct Labelled {
    #[jsonld(property = "https://example.com/name", container = "language")]
    pub name: HashMap<String, String>,
    #[jsonld(property = "https://example.com/note", container = "language")]
    pub note: BTreeMap<String, String>,
    #[jsonld(property = "https://example.com/absent", container = "language")]
    pub absent: Option<BTreeMap<String, String>>,
}

fn main() {
    let r = Labelled {
        name: HashMap::from([
            ("fr".to_string(), "nom".to_string()),
            ("en".to_string(), "name".to_string()),
            ("de".to_string(), "Name".to_string()),
        ]),
        note: BTreeMap::from([("en".to_string(), "note".to_string())]),
        absent: None,
    };
    let v: serde_json::Value = r.expand();
    let obj = v.as_object().unwrap();
    // Sorted by language tag, so a HashMap's iteration order cannot leak into
    // the output.
    assert_eq!(
        obj["https://example.com/name"],
        serde_json::json!([
            {"@value": "Name", "@language": "de"},
            {"@value": "name", "@language": "en"},
            {"@value": "nom", "@language": "fr"}
        ])
    );
    assert_eq!(
        obj["https://example.com/note"],
        serde_json::json!([{"@value": "note", "@language": "en"}])
    );
    assert!(!obj.contains_key("https://example.com/absent"));
}
