use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(
    type = "ngsi:EntityTypeInfo",
    prefix(ngsi = "https://uri.etsi.org/ngsi-ld/")
)]
pub struct Etti {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "ngsi:typeName")]
    pub type_name: String,
}

fn main() {
    let e = Etti {
        id: "urn:e:1".into(),
        type_name: "Vehicle".into(),
    };
    let v: serde_json::Value = e.expand();
    assert_eq!(
        v["@type"],
        serde_json::json!(["https://uri.etsi.org/ngsi-ld/EntityTypeInfo"])
    );
    assert_eq!(
        v["https://uri.etsi.org/ngsi-ld/typeName"],
        serde_json::json!([{"@value": "Vehicle"}])
    );
}
