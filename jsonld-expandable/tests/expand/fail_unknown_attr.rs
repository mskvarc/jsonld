use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct UnknownAttr {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(proprety = "https://example.com/name")]
    pub name: String,
}

fn main() {}
