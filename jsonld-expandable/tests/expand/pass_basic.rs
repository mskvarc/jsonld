use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(type = "https://uri.etsi.org/ngsi-ld/EntityTypeInfo")]
pub struct EntityTypeInfo {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://uri.etsi.org/ngsi-ld/typeName")]
    pub type_name: String,

    #[jsonld(property = "https://uri.etsi.org/ngsi-ld/entityCount")]
    pub entity_count: i64,
}

fn main() {
    let info = EntityTypeInfo {
        id: "urn:ngsi-ld:EntityTypeInfo:Vehicle".to_string(),
        type_name: "Vehicle".to_string(),
        entity_count: 42,
    };
    let expanded: serde_json::Value = info.expand();
    let obj = expanded.as_object().unwrap();
    assert_eq!(obj["@id"], "urn:ngsi-ld:EntityTypeInfo:Vehicle");
    assert_eq!(obj["@type"], serde_json::json!(["https://uri.etsi.org/ngsi-ld/EntityTypeInfo"]));
    assert_eq!(
        obj["https://uri.etsi.org/ngsi-ld/typeName"],
        serde_json::json!([{"@value": "Vehicle"}])
    );
    assert_eq!(
        obj["https://uri.etsi.org/ngsi-ld/entityCount"],
        serde_json::json!([{"@value": 42}])
    );
}
