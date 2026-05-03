use iri_rs::Iri;

mod vocab {
    jsonld_vocab::generate! {
        contexts: [
            "tests/fixtures/two_files_a.jsonld",
            "tests/fixtures/two_files_b.jsonld",
        ]
    }
}

fn main() {
    let _: Iri<&'static str> = vocab::prefix::EX;
    let _: Iri<&'static str> = vocab::expanded::properties::NAME;
    let _: Iri<&'static str> = vocab::expanded::properties::VALUE;
    assert_eq!(vocab::TERM_COUNT, 2);
}
