//! Regression tests for silent output-corruption bugs found during the
//! pre-publish review. These paths are not covered by the W3C conformance
//! suite.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use contextual::IntoRefWithContext;
use iri_rs::IriBuf;
use jsonld_core::{
    Container,
    ContainerKind,
    ExpandedDocument,
    Id,
    Indexed,
    Node,
    Object,
    ValidId,
    object::{IndexedEntryKeyRef, List, value},
};
use jsonld_syntax::IntoJsonWithContext;
use rdfx::{BlankIdBuf, vocabulary::no_vocabulary};

/// `["@graph", "@set"]` + `@index` must map to graph+index+set, not
/// graph+id+set.
#[test]
fn graph_set_with_index_maps_to_graph_index_set() {
    assert_eq!(Container::GraphSet.with(ContainerKind::Index), Some(Container::GraphIndexSet));
}

/// The `@included` entry must serialize under the `"@included"` key (it used
/// to come out as `"@include"`, which is not a JSON-LD keyword).
#[test]
fn node_included_serializes_as_included_keyword() {
    let mut node: Node<IriBuf, BlankIdBuf> = Node::new();
    node.set_included(Some(vec![Indexed::none(Node::new())]));

    let json = node.into_json_with(no_vocabulary());
    let object = json.as_object().unwrap();

    assert!(object.get_unique("@included").unwrap().is_some());
    assert!(object.get_unique("@include").unwrap().is_none());
}

/// The `@index` entry key must render as `"@index"` (it used to render as
/// `"@value"`).
#[test]
fn indexed_entry_key_renders_as_index_keyword() {
    let key: IndexedEntryKeyRef<'_, IriBuf, BlankIdBuf> = IndexedEntryKeyRef::Index;
    assert_eq!(key.into_ref_with(no_vocabulary()), "@index");
}

/// `FragmentRef::is_json_object` must report the JSON shape of the fragment,
/// not whether it is an array (copy-paste bug from `is_json_array`).
#[test]
fn value_fragment_json_object_is_json_object() {
    let json = jstrict::Value::Object(jstrict::Object::new());
    let fragment = jsonld_core::object::FragmentRef::<IriBuf, BlankIdBuf>::ValueFragment(value::FragmentRef::JsonFragment(jstrict::FragmentRef::Value(&json)));

    assert!(fragment.is_json_object());
    assert!(!fragment.is_json_array());
}

/// A minimal reverse interpretation for `from_interpreted_quads` tests:
/// resources are indices into per-kind tables.
mod interpretation {
    use std::borrow::Cow;

    use iri_rs::{Iri, IriBuf};
    use rdfx::{
        CowLiteral,
        LiteralRef,
        interpretation::{Interpretation, ReverseBlankIdInterpretation, ReverseInterpretation, ReverseIriInterpretation, ReverseLiteralInterpretation},
    };

    #[derive(Default)]
    pub struct TestInterpretation {
        iris: Vec<Vec<IriBuf>>,
        blank_ids: Vec<Vec<rdfx::BlankIdBuf>>,
        literals: Vec<Vec<rdfx::Literal>>,
    }

    impl TestInterpretation {
        fn new_resource(&mut self) -> usize {
            self.iris.push(Vec::new());
            self.blank_ids.push(Vec::new());
            self.literals.push(Vec::new());
            self.iris.len() - 1
        }

        pub fn iri(&mut self, iri: &str) -> usize {
            let r = self.new_resource();
            self.iris[r].push(IriBuf::new(iri.to_string()).unwrap());
            r
        }

        /// An anonymous resource (no IRI, no literal): a blank list node.
        pub fn anonymous(&mut self) -> usize {
            self.new_resource()
        }

        pub fn literal(&mut self, value: &str) -> usize {
            let r = self.new_resource();
            self.literals[r].push(rdfx::Literal::new(value, rdfx::LiteralType::Any(rdfx::Datatype::xsd_string())));
            r
        }
    }

    impl Interpretation for TestInterpretation {
        type Resource = usize;

        fn iri(&self, iri: Iri<&str>) -> Option<usize> {
            self.iris.iter().position(|iris| iris.iter().any(|i| i.as_str() == iri.as_str()))
        }

        fn literal<'a>(&self, literal: impl Into<LiteralRef<'a>>) -> Option<usize> {
            let literal = literal.into();
            self.literals.iter().position(|literals| literals.iter().any(|l| l.as_ref() == literal))
        }
    }

    impl ReverseIriInterpretation for TestInterpretation {
        type Iri = IriBuf;
        type Iris<'a> = std::slice::Iter<'a, IriBuf>;

        fn iris_of<'a>(&'a self, id: &'a usize) -> Self::Iris<'a> {
            self.iris[*id].iter()
        }
    }

    impl ReverseBlankIdInterpretation for TestInterpretation {
        type BlankId = rdfx::BlankIdBuf;
        type BlankIds<'a> = std::slice::Iter<'a, rdfx::BlankIdBuf>;

        fn blank_ids_of<'a>(&'a self, id: &'a usize) -> Self::BlankIds<'a> {
            self.blank_ids[*id].iter()
        }
    }

    impl ReverseLiteralInterpretation for TestInterpretation {
        type Literal = rdfx::Literal;
        type Literals<'a> = std::slice::Iter<'a, rdfx::Literal>;

        fn literals_of<'a>(&'a self, id: &'a usize) -> Self::Literals<'a> {
            self.literals[*id].iter()
        }
    }

    impl ReverseInterpretation for TestInterpretation {
        type Iris<'a> = std::iter::Map<std::slice::Iter<'a, IriBuf>, fn(&'a IriBuf) -> Cow<'a, IriBuf>>;
        type Literals<'a> = std::iter::Map<std::slice::Iter<'a, rdfx::Literal>, fn(&'a rdfx::Literal) -> CowLiteral<'a>>;

        fn iris_of<'a>(&'a self, resource: &'a usize) -> <Self as ReverseInterpretation>::Iris<'a> {
            self.iris[*resource].iter().map(Cow::Borrowed)
        }

        fn literals_of<'a>(&'a self, resource: &'a usize) -> <Self as ReverseInterpretation>::Literals<'a> {
            self.literals[*resource].iter().map(rdfx::Literal::as_cow)
        }
    }
}

/// `from_interpreted_quads` must fold well-formed RDF lists back into
/// `@list` values; `reverse_rest` used to record the predicate instead of
/// the subject, so the folding never fired.
#[test]
fn from_rdf_folds_lists() {
    use interpretation::TestInterpretation;
    use rdfx::GeneralizedQuad as Quad;

    let mut interpretation = TestInterpretation::default();
    let s = interpretation.iri("http://example.com/s");
    let p = interpretation.iri("http://example.com/p");
    let first = interpretation.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#first");
    let rest = interpretation.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");
    let nil = interpretation.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");
    let b0 = interpretation.anonymous();
    let b1 = interpretation.anonymous();
    let v0 = interpretation.literal("v0");
    let v1 = interpretation.literal("v1");

    let quads = [
        Quad(&s, &p, &b0, None),
        Quad(&b0, &first, &v0, None),
        Quad(&b0, &rest, &b1, None),
        Quad(&b1, &first, &v1, None),
        Quad(&b1, &rest, &nil, None),
    ];

    let document = ExpandedDocument::from_interpreted_quads(no_vocabulary(), &interpretation, quads).unwrap();

    assert_eq!(document.len(), 1);
    let node = document.iter().next().unwrap().as_node().unwrap();
    let (_, objects) = node.properties().iter().next().unwrap();
    let list = objects[0].as_list().expect("property value should be folded into a @list");

    let values: Vec<_> = list.iter().map(|o| o.as_value().unwrap().as_str().unwrap().to_string()).collect();
    assert_eq!(values, ["v0", "v1"]);
}

/// Literals must survive `ld-core` serialization.
///
/// `LinkedDataResource for Value` has to report an interpreted term; a
/// `Uninterpreted(None)` stub compiles and silently drops every literal.
#[test]
fn linked_data_serialization_keeps_literals() {
    let mut node: Node<IriBuf, rdfx::BlankIdBuf> = Node::with_id(Id::Valid(ValidId::Iri(IriBuf::new("http://example.com/s".to_string()).unwrap())));
    node.insert(
        Id::Valid(ValidId::Iri(IriBuf::new("http://example.com/p".to_string()).unwrap())),
        Indexed::none(Object::Value(jsonld_core::Value::Literal(
            jsonld_core::object::value::Literal::String("hello".into()),
            None,
        ))),
    );

    let quads = ld_core::to_lexical_quads(rdfx::generator::Blank::new(), &node).unwrap();

    assert_eq!(quads.len(), 1);
    let object = &quads[0].2;
    let literal = object.as_literal().expect("the literal must not be dropped");
    assert_eq!(literal.as_str(), "hello");
}

/// Traversal must descend into `@list` contents; `blank_ids` used to miss
/// every blank node identifier inside a list.
#[test]
fn traverse_descends_into_lists() {
    let blank = BlankIdBuf::new("_:b0".to_string()).unwrap();

    let mut node: Node<IriBuf, BlankIdBuf> = Node::new();
    node.insert(
        Id::Valid(ValidId::Iri(IriBuf::new("http://example.com/p".to_string()).unwrap())),
        Indexed::none(Object::List(List::new(vec![Indexed::none(Object::node(Node::with_id(Id::Valid(
            ValidId::Blank(blank.clone()),
        ))))]))),
    );

    let mut document = ExpandedDocument::new();
    document.insert(Indexed::none(Object::node(node)));

    assert!(document.blank_ids().contains(&blank));
}
