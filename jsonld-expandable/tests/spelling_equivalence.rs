//! Old NGSI-flavored attribute spellings must produce the same expanded
//! output as the new spec-aligned spellings. This is a single integration
//! test that compares two derived structs side-by-side.

use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;
use serde_json::Value;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/T")]
pub struct OldSpelling {
    #[jsonld(id)]
    pub id: String,
    #[jsonld(property = "https://example.com/cat", vocab)]
    pub cat: String,
    #[jsonld(property = "https://example.com/cats", vocab_vec)]
    pub cats: Vec<String>,
    #[jsonld(property = "https://example.com/tags", list)]
    pub tags: Vec<String>,
    #[jsonld(
        property = "https://example.com/at",
        typed_value,
        datatype = "http://www.w3.org/2001/XMLSchema#dateTime"
    )]
    pub at: String,
}

#[derive(Expandable)]
#[jsonld(type = "https://example.com/T")]
pub struct NewSpelling {
    #[jsonld(id)]
    pub id: String,
    #[jsonld(property = "https://example.com/cat", coerce = "@id")]
    pub cat: String,
    #[jsonld(property = "https://example.com/cats", coerce = "@id", vec)]
    pub cats: Vec<String>,
    #[jsonld(property = "https://example.com/tags", container = "list")]
    pub tags: Vec<String>,
    #[jsonld(
        property = "https://example.com/at",
        coerce = "http://www.w3.org/2001/XMLSchema#dateTime"
    )]
    pub at: String,
}

#[test]
fn old_and_new_spellings_produce_identical_output() {
    let id = "urn:t:1".to_string();
    let cat = "https://example.com/Cat".to_string();
    let cats = vec!["https://example.com/A".into(), "https://example.com/B".into()];
    let tags = vec!["x".to_string(), "y".to_string()];
    let at = "2026-05-03T10:00:00Z".to_string();

    let o: Value = OldSpelling {
        id: id.clone(),
        cat: cat.clone(),
        cats: cats.clone(),
        tags: tags.clone(),
        at: at.clone(),
    }
    .expand();
    let n: Value = NewSpelling { id, cat, cats, tags, at }.expand();

    assert_eq!(o, n);
}
