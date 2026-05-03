jsonld_vocab::generate! {
    contexts: [
        "tests/fixtures/conflicting_prefix_a.jsonld",
        "tests/fixtures/conflicting_prefix_b.jsonld",
    ]
}

fn main() {}
