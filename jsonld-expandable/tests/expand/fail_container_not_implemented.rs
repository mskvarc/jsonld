use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct ContainerNotImplemented {
    #[jsonld(property = "https://example.com/byId", container = "id")]
    pub by_id: Vec<String>,
}

fn main() {}
