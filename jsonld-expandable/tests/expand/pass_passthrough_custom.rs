use jsonld_expandable::Expandable;
use jsonld_expandable_core::{Expandable as _, JsonValue};

pub struct OneOrMany<T>(pub Vec<T>);

impl<T: AsRef<str>> jsonld_expandable_core::Expandable for OneOrMany<T> {
    fn expand<V: JsonValue>(&self) -> V {
        V::array(self.0.iter().map(|item| {
            V::object(::std::iter::once((
                "@id".to_string(),
                V::string(item.as_ref()),
            )))
        }))
    }
}

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Rel")]
pub struct Rel {
    #[jsonld(id)]
    pub id: String,
    #[jsonld(property = "https://example.com/object", custom)]
    pub object: OneOrMany<String>,
    #[jsonld(property = "https://example.com/maybeObject", custom)]
    pub maybe_object: Option<OneOrMany<String>>,
}

fn main() {
    let r = Rel {
        id: "urn:r:1".to_string(),
        object: OneOrMany(vec![
            "urn:e:1".to_string(),
            "urn:e:2".to_string(),
        ]),
        maybe_object: Some(OneOrMany(vec!["urn:e:3".to_string()])),
    };
    let v: serde_json::Value = r.expand();
    let obj = v.as_object().unwrap();
    // Custom expand output is inserted verbatim — no extra `[ ... ]` wrap.
    assert_eq!(
        obj["https://example.com/object"],
        serde_json::json!([{"@id": "urn:e:1"}, {"@id": "urn:e:2"}])
    );
    assert_eq!(
        obj["https://example.com/maybeObject"],
        serde_json::json!([{"@id": "urn:e:3"}])
    );
}
