//! Processed contexts and expanded objects shared by the unit tests of the
//! public single-value entry points.
#![allow(clippy::unwrap_used)]

use crate::{CompactFragment, Options, compact_iri_with, values_as_array};
use jsonld_context_processing::Process;
use jsonld_core::{Container, Context, Id, Indexed, NoLoader, Object, Term, TryFromJson};
use jsonld_syntax::{Parse, TryFromJson as _};
use rdfx::{
    BlankIdBuf,
    IriBuf,
    vocabulary::{no_vocabulary, no_vocabulary_mut},
};

/// Active context of the unit tests.
pub(crate) type TestContext = Context<IriBuf, BlankIdBuf>;

/// Expanded object of the unit tests.
pub(crate) type TestObject = Indexed<Object<IriBuf, BlankIdBuf>>;

/// A context defining one term per IRI and no map containers, so that every
/// value of a property compacts under a single key.
pub(crate) const VALUE_CONTEXT: &str = r#"{
    "@vocab": "https://vocab.example/",
    "ex": "https://example.com/",
    "xsd": "http://www.w3.org/2001/XMLSchema#",
    "id": "@id",
    "type": "@type",
    "value": "@value",
    "lang": "@language",
    "Building": "https://example.com/Building",
    "count": { "@id": "https://example.com/count", "@type": "xsd:integer" },
    "label": { "@id": "https://example.com/label", "@language": "en" },
    "text": "https://example.com/text",
    "object": { "@id": "https://example.com/object", "@type": "@id" },
    "objectVocab": { "@id": "https://example.com/objectVocab", "@type": "@vocab" },
    "json": { "@id": "https://example.com/json", "@type": "@json" },
    "tags": { "@id": "https://example.com/tags", "@container": "@set" }
}"#;

/// Parses a JSON document.
pub(crate) fn json(source: &str) -> jstrict::Value {
    jsonld_syntax::Value::parse_str(source).unwrap().0
}

/// Processes `source` as the `@context` of a document.
pub(crate) async fn processed_context(source: &str) -> TestContext {
    let local = jsonld_syntax::context::Context::try_from_json(&json(source)).unwrap();
    local.process(no_vocabulary_mut(), &NoLoader, None).await.unwrap().into_processed()
}

/// Reads an expanded object from its JSON form.
pub(crate) fn expanded_object(source: &str) -> TestObject {
    TestObject::try_from_json_in(no_vocabulary_mut(), json(source)).unwrap()
}

/// Parses an IRI.
pub(crate) fn iri(source: &str) -> IriBuf {
    IriBuf::new(source.to_owned()).unwrap()
}

/// The term `iri` as IRI compaction takes it.
pub(crate) fn iri_term(source: &str) -> Term<IriBuf, BlankIdBuf> {
    Term::Id(Id::iri(iri(source)))
}

/// Compacts a node holding `value` as the only value of `property` with the
/// full compaction algorithm, and returns the key the property compacted to
/// together with what was written under it.
pub(crate) async fn compact_one_value_document(context: &TestContext, property: &str, value: &str) -> (String, jstrict::Value) {
    let document = expanded_object(&format!(r#"{{ "{property}": [{value}] }}"#));
    let compacted = document
        .compact_fragment_full(no_vocabulary_mut(), context, context, None, &NoLoader, Options::default())
        .await
        .unwrap();

    let mut entries = compacted.into_object().unwrap().into_iter();
    let entry = entries.next().unwrap();
    assert!(entries.next().is_none(), "the document has a single property");

    let key = compact_iri_with(
        no_vocabulary(),
        context,
        &iri_term(property),
        &expanded_object(value),
        true,
        false,
        Options::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(entry.key.as_str(), &*key, "the property compacts to the key term selection picks");

    (key.to_string(), entry.value)
}

/// Wraps `compacted`, the compacted form of a single value written under
/// `key`, the way the compaction algorithm adds it to its node object.
pub(crate) fn as_written_under(context: &TestContext, key: &str, compacted: jstrict::Value) -> jstrict::Value {
    let container = context.get(key).map_or(Container::None, |definition| definition.container());
    if values_as_array(container, key, Options::default().compact_arrays) {
        jstrict::Value::Array(vec![compacted].into())
    } else {
        compacted
    }
}
