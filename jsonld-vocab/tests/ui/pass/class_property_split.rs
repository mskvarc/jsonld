use iri_rs::Iri;

mod vocab {
    jsonld_vocab::generate! {
        contexts: ["tests/fixtures/classes_and_properties.jsonld"]
    }
}

fn main() {
    let _: Iri<&'static str> = vocab::expanded::classes::PROPERTY;
    let _: Iri<&'static str> = vocab::expanded::classes::GEO_PROPERTY;
    let _: Iri<&'static str> = vocab::expanded::classes::RELATIONSHIP;
    let _: Iri<&'static str> = vocab::expanded::properties::PROPERTY;
    let _: Iri<&'static str> = vocab::expanded::properties::CREATED_AT;
    let _: Iri<&'static str> = vocab::expanded::properties::OBSERVED_AT;
    let _: Iri<&'static str> = vocab::expanded::properties::VALUE;

    assert_ne!(
        vocab::expanded::classes::PROPERTY.as_str(),
        vocab::expanded::properties::PROPERTY.as_str()
    );
    assert_eq!(
        vocab::expanded::classes::PROPERTY.as_str(),
        "https://example.org/ns#Property"
    );
    assert_eq!(
        vocab::expanded::properties::PROPERTY.as_str(),
        "https://example.org/ns#property"
    );

    assert_eq!(vocab::compact::classes::PROPERTY, "Property");
    assert_eq!(vocab::compact::properties::PROPERTY, "property");
}
