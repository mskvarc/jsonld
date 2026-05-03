use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "NotAnIRI")]
pub struct BadType {
    #[jsonld(id)]
    pub id: String,
}

fn main() {}
