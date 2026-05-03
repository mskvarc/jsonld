use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct VecWithoutNested {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/items", vec)]
    pub items: Vec<String>,
}

fn main() {}
