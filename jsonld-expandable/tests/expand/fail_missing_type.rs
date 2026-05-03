use jsonld_expandable::Expandable;

#[derive(Expandable)]
pub struct MissingType {
    #[jsonld(id)]
    pub id: String,
}

fn main() {}
