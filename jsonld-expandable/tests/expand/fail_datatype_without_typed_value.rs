use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct DatatypeWithoutTypedValue {
    #[jsonld(property = "https://example.com/when", datatype = "http://www.w3.org/2001/XMLSchema#dateTime")]
    pub when: String,
}

fn main() {}
