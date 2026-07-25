use jsonld_expandable::Expandable;
use jsonld_expandable_core::Expandable as _;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/ListRel")]
pub struct ListRel {
    #[jsonld(property = "https://example.com/objectList", list, id_ref)]
    pub object_list: Vec<String>,
    #[jsonld(property = "https://example.com/prevObjectList", list, id_ref)]
    pub prev_object_list: Option<Vec<String>>,
    #[jsonld(property = "https://example.com/valueList", list)]
    pub value_list: Vec<i64>,
}

fn main() {
    let r = ListRel {
        object_list: vec!["urn:e:1".to_string(), "urn:e:2".to_string()],
        prev_object_list: Some(vec!["urn:e:0".to_string()]),
        value_list: vec![1, 2, 3],
    };
    let v: serde_json::Value = r.expand();
    let obj = v.as_object().unwrap();
    // list + id_ref: [{"@list": [{"@id": ...}, ...]}]
    assert_eq!(
        obj["https://example.com/objectList"],
        serde_json::json!([
            {"@list": [{"@id": "urn:e:1"}, {"@id": "urn:e:2"}]}
        ])
    );
    assert_eq!(
        obj["https://example.com/prevObjectList"],
        serde_json::json!([
            {"@list": [{"@id": "urn:e:0"}]}
        ])
    );
    // list alone: [{"@list": <to_json_value(Vec<i64>)>}]
    assert_eq!(
        obj["https://example.com/valueList"],
        serde_json::json!([{"@list": [1, 2, 3]}])
    );
}
