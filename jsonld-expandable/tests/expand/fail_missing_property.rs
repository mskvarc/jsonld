use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct MissingProperty {
    pub name: String,
}

fn main() {}
