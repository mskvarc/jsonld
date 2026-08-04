use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct TypedValueWithoutDatatype {
    #[jsonld(property = "https://example.com/when", typed_value)]
    pub when: String,
}

fn main() {}
