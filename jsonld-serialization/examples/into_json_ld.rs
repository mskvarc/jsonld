#![allow(clippy::expect_used)]
use contextual::WithContext;
use jstrict::Print;
use jsonld_serialization::serialize;

#[derive(ld_core::Serialize)]
#[ld(prefix("ex" = "http://example.org/"))]
struct Foo {
    #[ld("ex:name")]
    name: String,

    #[ld("ex:email")]
    email: String,
}

fn main() {
    let value = Foo {
        name: "John Smith".to_string(),
        email: "john.smith@example.org".to_string(),
    };

    let json = serialize(&value).expect("serialization failed");
    eprintln!("{}", json.with(&()).pretty_print());
}
