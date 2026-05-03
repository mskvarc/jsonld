use iri_rs::Iri;

mod vocab {
    jsonld_vocab::generate! {
        contexts: ["tests/fixtures/simple.jsonld"]
    }
}

fn main() {
    let _: Iri<&'static str> = vocab::prefix::EX;
    let _: Iri<&'static str> = vocab::expanded::properties::NAME;
    let _: Iri<&'static str> = vocab::expanded::properties::CREATED_AT;
    let _: Iri<&'static str> = vocab::expanded::properties::VALUE;

    assert_eq!(
        vocab::expanded::properties::NAME.as_str(),
        "https://example.org/ns#name"
    );
    assert_eq!(
        vocab::compact_to_expanded("createdAt"),
        Some("https://example.org/ns#createdAt")
    );
    assert_eq!(
        vocab::expanded_to_compact("https://example.org/ns#value"),
        Some("value")
    );
    assert_eq!(vocab::TERM_COUNT, 3);
}
