use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct IdWithProperty {
    #[jsonld(id, property = "https://example.com/id")]
    pub id: String,
}

fn main() {}
