use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/MyEnum")]
pub enum NotAStruct {
    A,
    B,
}

fn main() {}
