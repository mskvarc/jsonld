use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct Conflicting {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/value", nested, coerce = "@id")]
    pub value: String,
}

fn main() {}
