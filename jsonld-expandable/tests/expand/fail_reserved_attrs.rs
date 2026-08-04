use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Thing")]
pub struct ReservedAttrs {
    #[jsonld(property = "https://example.com/inner", nest)]
    pub inner: String,
}

fn main() {}
