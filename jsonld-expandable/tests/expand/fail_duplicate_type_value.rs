use jsonld_expandable::Expandable;

#[derive(Expandable)]
pub struct DuplicateTypeValue {
    #[jsonld(type_value)]
    pub ty_a: Vec<String>,

    #[jsonld(type_value)]
    pub ty_b: Vec<String>,
}

fn main() {}
