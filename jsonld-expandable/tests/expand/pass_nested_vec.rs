use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Detail")]
pub struct Detail {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/value")]
    pub value: String,
}

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Parent")]
pub struct Parent {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/details", nested, vec)]
    pub details: Vec<Detail>,

    #[jsonld(property = "https://example.com/child", nested)]
    pub child: Detail,

    #[jsonld(property = "https://example.com/tags", list)]
    pub tags: Vec<String>,

    #[jsonld(property = "https://example.com/category", vocab)]
    pub category: String,

    #[jsonld(property = "https://example.com/types", vocab_vec)]
    pub types: Vec<String>,

    #[jsonld(skip)]
    pub internal: String,
}

fn main() {
    let p = Parent {
        id: "urn:p".into(),
        details: vec![Detail { id: "urn:d1".into(), value: "v".into() }],
        child: Detail { id: "urn:d0".into(), value: "x".into() },
        tags: vec!["a".into(), "b".into()],
        category: "https://example.com/Cat".into(),
        types: vec!["https://example.com/T1".into(), "https://example.com/T2".into()],
        internal: "hidden".into(),
    };
    let v: serde_json::Value = p.expand();
    let o = v.as_object().unwrap();
    assert!(o.get("https://example.com/internal").is_none());
    assert_eq!(
        o["https://example.com/types"],
        serde_json::json!([
            {"@id": "https://example.com/T1"},
            {"@id": "https://example.com/T2"}
        ])
    );
    assert_eq!(
        o["https://example.com/category"],
        serde_json::json!([{"@id": "https://example.com/Cat"}])
    );
    assert_eq!(
        o["https://example.com/tags"],
        serde_json::json!([{"@list": ["a", "b"]}])
    );
    assert_eq!(o["https://example.com/details"][0]["@id"], "urn:d1");
    assert_eq!(o["https://example.com/child"][0]["@id"], "urn:d0");
}
