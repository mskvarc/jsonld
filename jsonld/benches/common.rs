#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
#![allow(dead_code)]

use iri_rs::iri;
use jsonld::{
    BlankIdBuf,
    ExpandedDocument,
    IriBuf,
    JsonLdProcessor,
    NoLoader,
    RemoteDocument,
    context_processing::{Process, ProcessedOwned},
    rdfx::vocabulary::{IndexVocabulary, IriIndex, IriVocabularyMut},
    syntax::{Parse, TryFromJson, Value, context::Context as SyntaxContext},
};

pub struct Scenario {
    pub name: &'static str,
    pub doc: String,
    pub context: String,
}

fn scenario(name: &'static str, doc: String, context: String) -> Scenario {
    Scenario { name, doc, context }
}

pub fn parse_remote_doc(doc: &str) -> RemoteDocument {
    let value = Value::parse_str(doc).expect("doc parse").0;
    RemoteDocument::new(Some(iri!("https://bench.example.com/doc.jsonld").into()), None, value)
}

/// Creates an empty interning vocabulary seeded with the benchmark document
/// URL, so that the [`IriIndex`] held by a [`parse_remote_doc_indexed`]
/// document remains valid.
///
/// Seeding is what lets a benchmark reuse one parsed document across
/// iterations while handing each iteration a fresh vocabulary: the URL is
/// always the first IRI interned, so it always lands on the same index.
pub fn fresh_indexed_vocabulary() -> IndexVocabulary {
    let mut vocabulary = IndexVocabulary::new();
    let _ = vocabulary.insert(iri!("https://bench.example.com/doc.jsonld"));
    vocabulary
}

/// Parses `doc` into a remote document whose URL is interned in `vocabulary`.
///
/// The `IriBuf` document produced by [`parse_remote_doc`] expands against
/// `NoVocabulary`, where nothing is interned at all. Only this variant
/// exercises the configuration real consumers use, in which every IRI the
/// expansion sees becomes an index in a shared table.
pub fn parse_remote_doc_indexed(doc: &str, vocabulary: &mut IndexVocabulary) -> RemoteDocument<IriIndex> {
    let value = Value::parse_str(doc).expect("doc parse").0;
    let url = vocabulary.insert(iri!("https://bench.example.com/doc.jsonld"));
    RemoteDocument::new(Some(url), None, value)
}

pub fn parse_syntax_context(ctx: &str) -> SyntaxContext {
    let value = Value::parse_str(ctx).expect("ctx parse").0;
    SyntaxContext::try_from_json(&value).expect("ctx try_from_json")
}

pub async fn pre_expand(remote: &RemoteDocument) -> ExpandedDocument<IriBuf, BlankIdBuf> {
    remote.expand(&NoLoader).await.expect("pre-expand")
}

pub async fn pre_process_context(ctx: SyntaxContext) -> ProcessedOwned<IriBuf, BlankIdBuf> {
    let processed = ctx
        .process(
            jsonld::rdfx::vocabulary::no_vocabulary_mut(),
            &NoLoader,
            Some(iri!("https://bench.example.com/").into()),
        )
        .await
        .expect("ctx process");
    ProcessedOwned::new(ctx.clone(), processed.into_processed())
}

pub fn corpus() -> Vec<Scenario> {
    vec![
        simple_flat(),
        nested_objects(),
        deep_nested(50),
        very_deep_nested(200),
        wide_array_values(1_000),
        many_properties(100),
        many_entities(1_000),
        type_coercion_iri(),
        type_coercion_xsd(),
        container_list(),
        container_set(),
        container_language(),
        container_index(),
        container_id(),
        container_type(),
        container_graph(),
        type_scoped(),
        property_scoped(),
        protected_terms(),
        nested_keyword(),
        included_keyword(),
        reverse_property(),
        value_object_lang_dir(),
        multiple_types(),
        compact_iris_prefixes(),
        vocab_expansion(),
        graph_named(),
        nested_lists(),
        json_literal(),
        blank_nodes_many(100),
        mixed_realistic(),
        index_map_large(200),
        language_map_large(50),
        array_of_value_objects(500),
        nested_with_repeated_terms(20, 25),
        ngsi_ld_entity_collection(200),
    ]
}

/// Corpus for the IRI-resolution benchmarks.
///
/// This is the only shape in the whole corpus that reaches `iri-rs`'s resolver
/// in anger. `expand_iri` returns early for a value that already parses as an
/// absolute IRI, so a document built from absolute `@id`s — which is every
/// other scenario here — never calls `resolved` at all. Resolution happens only
/// when a value is a *relative* reference and the active context carries a
/// `@base`.
///
/// The three scenarios split by which branch of RFC 3986 §5.2.2 they take,
/// because the branches have very different costs and only some of them were
/// touched by the dot-segment fix:
///
/// - `clean` — relative-path references with no dot segments. The dominant
///   real-world shape, and the branch that always walked the path segment by
///   segment, before and after the fix.
/// - `dot_segments` — relative-path references containing `./` and `../`, so
///   the walker also has to unwind segments.
/// - `network_path` — `//host/path` references, one of the two branches the
///   fix changed, and the shape behind the `expand_t0062` conformance failure.
///
/// `absolute_control` is the same document with absolute `@id`s and no
/// `@base`, so `expand_iri` short-circuits and never calls the resolver. The
/// gap between it and `clean` is the share of expansion that IRI resolution
/// actually accounts for, which is what says whether a resolver change can move
/// JSON-LD at all.
pub fn base_relative_corpus() -> Vec<Scenario> {
    vec![
        base_absolute_control(300),
        base_relative_clean(300),
        base_relative_dot_segments(300),
        base_relative_network_path(300),
    ]
}

/// `@base` mirroring the deep document URL that real `@base` values take, and
/// the one the W3C `expand_t0062` test uses.
const DEEP_BASE: &str = "https://example.com/some/deep/directory/and/file";

/// Builds a `@graph` of `n` nodes whose `@id` and `link` are relative
/// references produced by `reference`, resolved against [`DEEP_BASE`].
fn base_relative_scenario(name: &'static str, n: usize, reference: impl Fn(usize, &str) -> String) -> Scenario {
    let context = format!(r#"{{"@base":"{DEEP_BASE}","@vocab":"https://ex.org/vocab/","link":{{"@id":"https://ex.org/vocab/link","@type":"@id"}}}}"#);
    let mut doc = format!(r#"{{"@context":{context},"@graph":["#);
    for index in 0..n {
        if index > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(
            r#"{{"@id":"{id}","title":"Node {index}","link":"{link}"}}"#,
            id = reference(index, "entity"),
            link = reference(index, "other"),
        ));
    }
    doc.push_str("]}");
    scenario(name, doc, context)
}

/// Control: identical shape, absolute `@id`s, no `@base`. `expand_iri` returns
/// before reaching the resolver, so this measures everything *except*
/// resolution.
fn base_absolute_control(n: usize) -> Scenario {
    let context = r#"{"@vocab":"https://ex.org/vocab/","link":{"@id":"https://ex.org/vocab/link","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{context},"@graph":["#);
    for index in 0..n {
        if index > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(
            r#"{{"@id":"{DEEP_BASE_DIR}/entity/{index}","title":"Node {index}","link":"{DEEP_BASE_DIR}/other/{index}"}}"#
        ));
    }
    doc.push_str("]}");
    scenario("base_absolute_control_300", doc, context.to_string())
}

/// Directory part of [`DEEP_BASE`], so the control's absolute IRIs come out the
/// same length as the resolved forms in the relative scenarios.
const DEEP_BASE_DIR: &str = "https://example.com/some/deep/directory/and";

fn base_relative_clean(n: usize) -> Scenario {
    base_relative_scenario("base_relative_clean_300", n, |index, kind| format!("{kind}/{index}"))
}

fn base_relative_dot_segments(n: usize) -> Scenario {
    base_relative_scenario("base_relative_dot_segments_300", n, |index, kind| format!("../{kind}/./nested/../{index}"))
}

fn base_relative_network_path(n: usize) -> Scenario {
    base_relative_scenario("base_relative_network_path_300", n, |index, kind| {
        format!("//cdn.example.com/{kind}/../item/{index}")
    })
}

/// Corpus for the interning-vocabulary benchmarks.
///
/// The general [`corpus`] runs everything against `NoVocabulary`, where nothing
/// is interned, so it says nothing about the cost of interning. These scenarios
/// are wide arrays of node objects — the shape where per-item interning cost
/// accumulates — and are the corpus against which any future concurrent or
/// batching design has to be judged.
pub fn vocabulary_corpus() -> Vec<Scenario> {
    vec![
        heavy_node_array(256),
        ngsi_ld_entity_collection(200),
        index_map_large(200),
        blank_nodes_many(100),
        array_of_value_objects(500),
    ]
}

/// Array of `n` node objects, each heavy enough that per-item expansion work
/// dominates per-item bookkeeping.
///
/// Each item contributes several distinct IRIs and a nested node, so expanding
/// this against an [`IndexVocabulary`] makes per-item interning cost visible.
fn heavy_node_array(n: usize) -> Scenario {
    let context = r#"{"@vocab":"https://ex.org/vocab/","author":{"@id":"https://ex.org/vocab/author","@type":"@id"},"cites":{"@id":"https://ex.org/vocab/cites","@type":"@id"},"about":{"@id":"https://ex.org/vocab/about","@type":"@id"},"license":{"@id":"https://ex.org/vocab/license","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@graph":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(
            r#"{{"@id":"https://ex.org/doc/{i}","@type":"https://ex.org/type/T{ty}","title":"Document {i}","revision":{i},"author":"https://ex.org/person/{au}","about":"https://ex.org/topic/{tp}","cites":["https://ex.org/doc/{c1}","https://ex.org/doc/{c2}"],"meta":{{"@id":"https://ex.org/meta/{i}","license":"https://ex.org/license/{li}","checksum":"sha256-{i}"}}}}"#,
            ty = i % 16,
            au = i % 64,
            tp = i % 32,
            c1 = (i + 1) % n,
            c2 = (i + 7) % n,
            li = i % 4,
        ));
    }
    doc.push_str("]}");
    scenario("heavy_node_array_256", doc, context.to_string())
}

fn simple_flat() -> Scenario {
    let context = r#"{"name":"http://xmlns.com/foaf/0.1/name","homepage":{"@id":"http://xmlns.com/foaf/0.1/homepage","@type":"@id"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://example.org/alice","name":"Alice","homepage":"https://example.org/alice/home"}}"#,
        ctx = context
    );
    scenario("simple_flat", doc, context.to_string())
}

fn nested_objects() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","knows":{"@id":"http://schema.org/knows"},"address":{"@id":"http://schema.org/address"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/alice","name":"Alice","address":{{"streetAddress":"1 Foo St","addressLocality":"Bar","postalCode":"12345"}},"knows":[{{"name":"Bob","address":{{"streetAddress":"2 Baz Ave","postalCode":"54321"}}}},{{"name":"Carol","address":{{"streetAddress":"3 Qux Rd","postalCode":"67890"}}}}]}}"#,
        ctx = context
    );
    scenario("nested_objects", doc, context.to_string())
}

fn deep_nested(depth: usize) -> Scenario {
    let context = r#"{"name":"http://ex.org/name","child":"http://ex.org/child"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"name":"root""#, ctx = context);
    for i in 0..depth {
        doc.push_str(&format!(r#","child":{{"name":"n{i}""#));
    }
    for _ in 0..depth {
        doc.push('}');
    }
    doc.push('}');
    scenario("deep_nested_50", doc, context.to_string())
}

fn very_deep_nested(depth: usize) -> Scenario {
    let mut s = deep_nested(depth);
    s.name = "very_deep_nested_200";
    s
}

fn wide_array_values(n: usize) -> Scenario {
    let context = r#"{"items":"http://ex.org/items"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""value-{i}""#));
    }
    doc.push_str("]}");
    scenario("wide_array_values_1000", doc, context.to_string())
}

fn many_properties(n: usize) -> Scenario {
    let mut ctx = String::from("{");
    for i in 0..n {
        if i > 0 {
            ctx.push(',');
        }
        ctx.push_str(&format!(r#""p{i}":"http://ex.org/p{i}""#));
    }
    ctx.push('}');
    let mut doc = format!(r#"{{"@context":{ctx},"#);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""p{i}":"v{i}""#));
    }
    doc.push('}');
    scenario("many_properties_100", doc, ctx)
}

fn many_entities(n: usize) -> Scenario {
    let context = r#"{"name":"http://xmlns.com/foaf/0.1/name","knows":{"@id":"http://xmlns.com/foaf/0.1/knows","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@graph":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        let next = (i + 1) % n;
        doc.push_str(&format!(
            r#"{{"@id":"https://ex.org/p{i}","name":"Person {i}","knows":"https://ex.org/p{next}"}}"#
        ));
    }
    doc.push_str("]}");
    scenario("many_entities_1000", doc, context.to_string())
}

fn type_coercion_iri() -> Scenario {
    let context = r#"{"author":{"@id":"http://schema.org/author","@type":"@id"},"creator":{"@id":"http://purl.org/dc/terms/creator","@type":"@id"},"editor":{"@id":"http://schema.org/editor","@type":"@id"},"reviewer":{"@id":"http://schema.org/reviewer","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@id":"https://ex.org/book","#, ctx = context);
    for (i, term) in ["author", "creator", "editor", "reviewer"].iter().enumerate() {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""{term}":["#));
        for j in 0..50 {
            if j > 0 {
                doc.push(',');
            }
            doc.push_str(&format!(r#""https://ex.org/{term}/{j}""#));
        }
        doc.push(']');
    }
    doc.push('}');
    scenario("type_coercion_iri", doc, context.to_string())
}

fn type_coercion_xsd() -> Scenario {
    let context = r#"{"date":{"@id":"http://schema.org/date","@type":"http://www.w3.org/2001/XMLSchema#dateTime"},"count":{"@id":"http://schema.org/count","@type":"http://www.w3.org/2001/XMLSchema#integer"},"price":{"@id":"http://schema.org/price","@type":"http://www.w3.org/2001/XMLSchema#decimal"},"active":{"@id":"http://schema.org/active","@type":"http://www.w3.org/2001/XMLSchema#boolean"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/r","date":"2026-04-27T10:00:00Z","count":"42","price":"19.99","active":"true"}}"#,
        ctx = context
    );
    scenario("type_coercion_xsd", doc, context.to_string())
}

fn container_list() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@list"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":["#, ctx = context);
    for i in 0..200 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""item-{i}""#));
    }
    doc.push_str("]}");
    scenario("container_list", doc, context.to_string())
}

fn container_set() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@set"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":["#, ctx = context);
    for i in 0..200 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""item-{i}""#));
    }
    doc.push_str("]}");
    scenario("container_set", doc, context.to_string())
}

fn container_language() -> Scenario {
    let context = r#"{"name":{"@id":"http://schema.org/name","@container":"@language"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"name":{{"en":"hello","fr":"salut","de":"hallo","es":"hola","it":"ciao","ja":"こんにちは","zh":"你好"}}}}"#,
        ctx = context
    );
    scenario("container_language", doc, context.to_string())
}

fn container_index() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@index"},"name":"http://schema.org/name"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":{{"#, ctx = context);
    for i in 0..50 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""key-{i}":{{"@id":"https://ex.org/{i}","name":"E{i}"}}"#));
    }
    doc.push_str("}}");
    scenario("container_index", doc, context.to_string())
}

fn container_id() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@id"},"name":"http://schema.org/name"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":{{"#, ctx = context);
    for i in 0..50 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""https://ex.org/{i}":{{"name":"E{i}"}}"#));
    }
    doc.push_str("}}");
    scenario("container_id", doc, context.to_string())
}

fn container_type() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@type"},"name":"http://schema.org/name"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"items":{{"http://schema.org/Person":{{"@id":"https://ex.org/p1","name":"Alice"}},"http://schema.org/Organization":{{"@id":"https://ex.org/o1","name":"Acme"}}}}}}"#,
        ctx = context
    );
    scenario("container_type", doc, context.to_string())
}

fn container_graph() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","graph":{"@id":"http://ex.org/graph","@container":"@graph"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/c","graph":{{"name":"Alice","age":30}}}}"#,
        ctx = context
    );
    scenario("container_graph", doc, context.to_string())
}

fn type_scoped() -> Scenario {
    let context = r#"{"@vocab":"http://ex.org/","Person":{"@id":"http://schema.org/Person","@context":{"name":"http://schema.org/name","age":"http://schema.org/age","email":"http://schema.org/email"}}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@type":"Person","name":"Alice","age":30,"email":"a@example.com"}}"#,
        ctx = context
    );
    scenario("type_scoped", doc, context.to_string())
}

fn property_scoped() -> Scenario {
    let context =
        r#"{"@vocab":"http://ex.org/","child":{"@id":"http://ex.org/child","@context":{"name":"http://schema.org/name","age":"http://schema.org/age"}}}"#;
    let doc = format!(r#"{{"@context":{ctx},"child":{{"name":"Bob","age":12}}}}"#, ctx = context);
    scenario("property_scoped", doc, context.to_string())
}

fn protected_terms() -> Scenario {
    let context = r#"{"@protected":true,"@vocab":"http://schema.org/","name":"http://schema.org/name","age":"http://schema.org/age"}"#;
    let doc = format!(r#"{{"@context":{ctx},"name":"Alice","age":30}}"#, ctx = context);
    scenario("protected_terms", doc, context.to_string())
}

fn nested_keyword() -> Scenario {
    let context =
        r#"{"@vocab":"http://ex.org/","details":"@nest","name":"http://schema.org/name","age":"http://schema.org/age","email":"http://schema.org/email"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"name":"Alice","details":{{"age":30,"email":"a@example.com"}}}}"#,
        ctx = context
    );
    scenario("nested_keyword", doc, context.to_string())
}

fn included_keyword() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","included":"@included"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/a","name":"A","included":[{{"@id":"https://ex.org/b","name":"B"}},{{"@id":"https://ex.org/c","name":"C"}}]}}"#,
        ctx = context
    );
    scenario("included_keyword", doc, context.to_string())
}

fn reverse_property() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","wroteBy":{"@reverse":"author"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/book","wroteBy":[{{"@id":"https://ex.org/alice"}},{{"@id":"https://ex.org/bob"}}]}}"#,
        ctx = context
    );
    scenario("reverse_property", doc, context.to_string())
}

fn value_object_lang_dir() -> Scenario {
    let context = r#"{"name":{"@id":"http://schema.org/name","@container":"@set"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"name":[{{"@value":"hello","@language":"en"}},{{"@value":"שלום","@language":"he","@direction":"rtl"}},{{"@value":"hola","@language":"es"}},{{"@value":"مرحبا","@language":"ar","@direction":"rtl"}}]}}"#,
        ctx = context
    );
    scenario("value_object_lang_dir", doc, context.to_string())
}

fn multiple_types() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/a","@type":["Person","Author","Researcher","Speaker","Educator"]}}"#,
        ctx = context
    );
    scenario("multiple_types", doc, context.to_string())
}

fn compact_iris_prefixes() -> Scenario {
    let context = r#"{"foaf":"http://xmlns.com/foaf/0.1/","schema":"http://schema.org/","dc":"http://purl.org/dc/terms/","ex":"http://example.org/","rdf":"http://www.w3.org/1999/02/22-rdf-syntax-ns#","rdfs":"http://www.w3.org/2000/01/rdf-schema#","xsd":"http://www.w3.org/2001/XMLSchema#"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"ex:alice","foaf:name":"Alice","schema:age":30,"dc:title":"About Alice","rdfs:label":"Alice"}}"#,
        ctx = context
    );
    scenario("compact_iris_prefixes", doc, context.to_string())
}

fn vocab_expansion() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/"}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/p","name":"Alice","age":30,"email":"a@example.com","telephone":"+1-555-1234","jobTitle":"Engineer"}}"#,
        ctx = context
    );
    scenario("vocab_expansion", doc, context.to_string())
}

fn graph_named() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@id":"https://ex.org/g1","@graph":["#, ctx = context);
    for i in 0..50 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#"{{"@id":"https://ex.org/p{i}","name":"P{i}","age":{}}}"#, 20 + i));
    }
    doc.push_str("]}");
    scenario("graph_named", doc, context.to_string())
}

fn nested_lists() -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@list"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"items":[{{"@list":[1,2,3,4,5]}},{{"@list":[6,7,8,9,10]}},{{"@list":[11,12,13,14,15]}}]}}"#,
        ctx = context
    );
    scenario("nested_lists", doc, context.to_string())
}

fn json_literal() -> Scenario {
    let context = r#"{"data":{"@id":"http://ex.org/data","@type":"@json"}}"#;
    let doc = format!(
        r#"{{"@context":{ctx},"data":{{"foo":"bar","nested":{{"a":1,"b":[2,3,4]}},"arr":[true,null,"x"]}}}}"#,
        ctx = context
    );
    scenario("json_literal", doc, context.to_string())
}

fn blank_nodes_many(n: usize) -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","knows":{"@id":"http://schema.org/knows","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@graph":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        let next = (i + 1) % n;
        doc.push_str(&format!(r#"{{"@id":"_:n{i}","name":"Node{i}","knows":"_:n{next}"}}"#));
    }
    doc.push_str("]}");
    scenario("blank_nodes_many_100", doc, context.to_string())
}

fn mixed_realistic() -> Scenario {
    let context = r#"{"@vocab":"http://schema.org/","foaf":"http://xmlns.com/foaf/0.1/","name":{"@id":"http://schema.org/name","@container":"@language"},"knows":{"@id":"http://schema.org/knows","@type":"@id"},"affiliations":{"@id":"http://schema.org/affiliation","@container":"@set"},"tags":{"@id":"http://schema.org/keywords","@container":"@list"},"docs":{"@id":"http://schema.org/document","@container":"@index"},"address":{"@id":"http://schema.org/address","@context":{"city":"http://schema.org/addressLocality","zip":"http://schema.org/postalCode"}}}"#;
    let mut doc = format!(
        r#"{{"@context":{ctx},"@id":"https://ex.org/alice","@type":["Person","foaf:Person"],"name":{{"en":"Alice","fr":"Alice","de":"Alice"}},"knows":["https://ex.org/bob","https://ex.org/carol","https://ex.org/dan"],"affiliations":[{{"@id":"https://ex.org/acme","name":{{"en":"Acme"}}}},{{"@id":"https://ex.org/foo","name":{{"en":"Foo Inc"}}}}],"tags":["rdf","jsonld","semweb"],"docs":{{"alpha":{{"@id":"https://ex.org/d1","name":{{"en":"Doc 1"}}}},"beta":{{"@id":"https://ex.org/d2","name":{{"en":"Doc 2"}}}}}},"address":{{"city":"Paris","zip":"75000"}}"#,
        ctx = context
    );
    doc.push_str(",\"colleagues\":[");
    for i in 0..20 {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#"{{"@id":"https://ex.org/c{i}","name":{{"en":"Colleague {i}"}}}}"#));
    }
    doc.push_str("]}");
    scenario("mixed_realistic", doc, context.to_string())
}

fn index_map_large(n: usize) -> Scenario {
    let context = r#"{"items":{"@id":"http://ex.org/items","@container":"@index"},"name":"http://schema.org/name","val":"http://ex.org/val"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"items":{{"#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""key-{i}":{{"@id":"https://ex.org/i{i}","name":"E{i}","val":{i}}}"#));
    }
    doc.push_str("}}");
    scenario("index_map_large_200", doc, context.to_string())
}

fn language_map_large(n: usize) -> Scenario {
    let context = r#"{"name":{"@id":"http://schema.org/name","@container":"@language"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"name":{{"#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""en-{i:03}":"hello-{i}""#));
    }
    doc.push_str("}}");
    scenario("language_map_large_50", doc, context.to_string())
}

fn array_of_value_objects(n: usize) -> Scenario {
    let context = r#"{"vals":{"@id":"http://ex.org/vals","@container":"@set"}}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"vals":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#"{{"@value":"v{i}","@language":"en","@direction":"ltr"}}"#));
    }
    doc.push_str("]}");
    scenario("array_of_value_objects_500", doc, context.to_string())
}

/// Representative NGSI-LD-shaped entity collection: a `@graph` of `n` entities,
/// each with `id`, `type`, `observedAt`, a GeoProperty `location`, and ~10
/// Property/Relationship attributes. Designed to stress per-entity expansion
/// work — both the wide-graph (`many_entities`-like) and per-node nesting
/// dimensions in a single fixture.
fn ngsi_ld_entity_collection(n: usize) -> Scenario {
    let context = r#"{"@vocab":"https://uri.etsi.org/ngsi-ld/","Property":"https://uri.etsi.org/ngsi-ld/Property","Relationship":"https://uri.etsi.org/ngsi-ld/Relationship","GeoProperty":"https://uri.etsi.org/ngsi-ld/GeoProperty","value":{"@id":"https://uri.etsi.org/ngsi-ld/hasValue"},"object":{"@id":"https://uri.etsi.org/ngsi-ld/hasObject","@type":"@id"},"observedAt":{"@id":"https://uri.etsi.org/ngsi-ld/observedAt","@type":"https://uri.etsi.org/ngsi-ld/DateTime"},"location":"https://uri.etsi.org/ngsi-ld/location","speed":"https://example.org/vehicle/speed","heading":"https://example.org/vehicle/heading","brakes":"https://example.org/vehicle/brakes","fuelLevel":"https://example.org/vehicle/fuelLevel","operator":"https://example.org/vehicle/operator","carriesGoods":"https://example.org/vehicle/carriesGoods","status":"https://example.org/vehicle/status","tagId":"https://example.org/vehicle/tagId","route":"https://example.org/vehicle/route","battery":"https://example.org/vehicle/battery"}"#;
    let mut doc = format!(r#"{{"@context":{ctx},"@graph":["#, ctx = context);
    for i in 0..n {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(
            r#"{{"id":"urn:ngsi-ld:Vehicle:V{i}","type":"Vehicle","observedAt":"2026-04-27T10:0{j}:00Z","location":{{"type":"GeoProperty","value":{{"type":"Point","coordinates":[{lon},{lat}]}}}},"speed":{{"type":"Property","value":{spd}}},"heading":{{"type":"Property","value":{hd}}},"brakes":{{"type":"Property","value":{br}}},"fuelLevel":{{"type":"Property","value":{fl}}},"operator":{{"type":"Relationship","object":"urn:ngsi-ld:Person:P{op}"}},"carriesGoods":{{"type":"Relationship","object":"urn:ngsi-ld:Goods:G{cg}"}},"status":{{"type":"Property","value":"active"}},"tagId":{{"type":"Property","value":"tag-{i}"}},"route":{{"type":"Relationship","object":"urn:ngsi-ld:Route:R{rt}"}},"battery":{{"type":"Property","value":{bt}}}}}"#,
            j = i % 10,
            lon = -180.0 + (i as f64 * 0.5),
            lat = -90.0 + (i as f64 * 0.25),
            spd = (i % 120) as i32,
            hd = (i % 360) as i32,
            br = i % 2 == 0,
            fl = ((i % 100) as f64) / 100.0,
            op = i % 50,
            cg = i % 25,
            rt = i % 10,
            bt = ((i % 100) as f64) / 100.0,
        ));
    }
    doc.push_str("]}");
    scenario("ngsi_ld_entity_collection_200", doc, context.to_string())
}

fn nested_with_repeated_terms(depth: usize, props: usize) -> Scenario {
    let mut ctx = String::from(r#"{"@vocab":"http://ex.org/","child":"http://ex.org/child""#);
    for i in 0..props {
        ctx.push_str(&format!(r#","p{i}":"http://ex.org/p{i}""#));
    }
    ctx.push('}');
    let mut doc = format!(r#"{{"@context":{ctx},"#);
    for i in 0..props {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(r#""p{i}":"v{i}""#));
    }
    for d in 0..depth {
        doc.push_str(r#","child":{"#);
        for i in 0..props {
            if i > 0 {
                doc.push(',');
            }
            doc.push_str(&format!(r#""p{i}":"v{d}-{i}""#));
        }
    }
    for _ in 0..depth {
        doc.push('}');
    }
    doc.push('}');
    scenario("nested_with_repeated_terms_20x25", doc, ctx)
}
