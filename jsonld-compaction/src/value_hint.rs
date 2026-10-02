use derive_where::derive_where;
use jsonld_core::{Context, Direction, Id, Indexed, LenientLangTag, Nullable, Object, Type, Value, object};
use std::fmt;

/// Describes a value to [IRI compaction][1] without materializing it.
///
/// IRI compaction picks the term for a property by looking at the value that
/// will be written under it: its kind (node, value, list or graph object),
/// its `@type`, `@language` and `@direction`, its `@id` and whether it has an
/// `@index`. [`compact_iri_with`][crate::compact_iri_with] reads all of that
/// off an expanded [`Indexed`] object; [`compact_iri_with_hint`] reads it off
/// this hint instead, so a caller walking its own data model can select terms
/// without first building `jsonld-core` objects.
///
/// A hint selects exactly the term the described object would select: both
/// entry points derive the container list and the type/language preference of
/// [term selection][2] through the same code.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#iri-compaction
/// [2]: https://www.w3.org/TR/json-ld-api/#term-selection
/// [`compact_iri_with_hint`]: crate::compact_iri_with_hint
#[derive_where(Clone, Copy)]
#[derive_where(Debug; T: fmt::Debug, B: fmt::Debug)]
pub struct ValueHint<'a, T, B> {
    /// Kind of the described value.
    pub kind: ValueKind<'a, T, B>,

    /// Whether the described value carries an `@index` entry.
    pub indexed: bool,
}

impl<'a, T, B> ValueHint<'a, T, B> {
    /// Creates a hint for a value without an `@index` entry.
    #[must_use]
    pub const fn new(kind: ValueKind<'a, T, B>) -> Self {
        Self { kind, indexed: false }
    }

    /// Creates a hint for a value carrying an `@index` entry.
    #[must_use]
    pub const fn indexed(kind: ValueKind<'a, T, B>) -> Self {
        Self { kind, indexed: true }
    }
}

/// Kind of a value described by a [`ValueHint`].
///
/// Each variant stands for one shape of expanded JSON-LD and carries exactly
/// the parts of it that IRI compaction reads.
#[derive_where(Clone, Copy)]
#[derive_where(Debug; T: fmt::Debug, B: fmt::Debug)]
pub enum ValueKind<'a, T, B> {
    /// A node object that is not a graph object, such as a node reference
    /// (`{"@id": ...}`), together with its `@id`, if any.
    ///
    /// When the `@id` compacts to a term whose IRI mapping is that same
    /// `@id`, term selection prefers `@vocab`-typed terms over `@id`-typed
    /// ones.
    Node {
        /// The node's `@id`.
        id: Option<&'a Id<T, B>>,
    },

    /// A graph object (a node object whose only entries are `@graph` and
    /// optionally `@id` and `@index`), together with its `@id`, if any.
    Graph {
        /// The graph's `@id`.
        id: Option<&'a Id<T, B>>,
    },

    /// A value object whose `@value` is a string, number, boolean or `null`,
    /// with no `@language` and no `@direction`.
    Literal {
        /// The value's `@type` IRI, if it has one.
        datatype: Option<&'a T>,
    },

    /// A value object holding a string with a `@language`, a `@direction`, or
    /// both.
    LangString {
        /// The value's `@language`.
        language: Option<&'a LenientLangTag>,

        /// The value's `@direction`.
        direction: Option<Direction>,
    },

    /// A value object of type `@json`.
    Json,

    /// A list object, with the kinds of its items in order.
    ///
    /// Only whether each item is a value object, and its type, language and
    /// direction if it is, take part in term selection; `@index` entries of
    /// the items do not.
    List(&'a [ValueKind<'a, T, B>]),

    /// The empty array an expanded property holds when it has no values.
    ///
    /// Selects the term an `@id`-less node object would.
    EmptyArray,
}

impl<'a, T, B> ValueKind<'a, T, B> {
    /// Returns the value object this kind describes, or `None` if it is not
    /// a value object.
    fn value_object(self) -> Option<ValueObjectFeatures<'a, T>> {
        match self {
            Self::Literal { datatype } => Some(ValueObjectFeatures::Literal(datatype)),
            Self::LangString { language, direction } => Some(ValueObjectFeatures::LangString { language, direction }),
            Self::Json => Some(ValueObjectFeatures::Json),
            Self::Node { .. } | Self::Graph { .. } | Self::List(_) | Self::EmptyArray => None,
        }
    }
}

/// Everything IRI compaction reads off the value it compacts a property for.
///
/// Derived either from an expanded object or from a [`ValueHint`]; deriving
/// both through this one type is what guarantees they select the same term.
pub(crate) struct ValueFeatures<'a, T, B> {
    /// Whether the value carries an `@index` entry.
    pub(crate) has_index: bool,

    /// Shape of the value.
    pub(crate) shape: ValueShape<'a, T, B>,
}

impl<'a, T, B> ValueFeatures<'a, T, B> {
    /// Returns the `@id` of the value, if it is a node or graph object with
    /// one.
    pub(crate) fn id(&self) -> Option<&'a Id<T, B>> {
        match &self.shape {
            ValueShape::Node { id } | ValueShape::Graph { id } => *id,
            ValueShape::Value(_) | ValueShape::List { .. } => None,
        }
    }

    /// Checks whether the value is a graph object.
    pub(crate) fn is_graph(&self) -> bool {
        matches!(self.shape, ValueShape::Graph { .. })
    }

    /// Checks whether the value is a list object without items.
    pub(crate) fn is_empty_list(&self) -> bool {
        matches!(self.shape, ValueShape::List { is_empty: true, .. })
    }
}

/// Shape of a value as IRI compaction distinguishes it.
pub(crate) enum ValueShape<'a, T, B> {
    /// A node object that is not a graph object.
    Node { id: Option<&'a Id<T, B>> },

    /// A graph object.
    Graph { id: Option<&'a Id<T, B>> },

    /// A value object.
    Value(ValueObjectFeatures<'a, T>),

    /// A list object, reduced to the type or language and direction its items
    /// have in common.
    List {
        common_type: Option<Type<T>>,
        common_lang_dir: Nullable<(Option<&'a LenientLangTag>, Option<Direction>)>,
        is_empty: bool,
    },
}

/// The parts of a value object IRI compaction reads.
pub(crate) enum ValueObjectFeatures<'a, T> {
    /// A literal, with its datatype IRI if any.
    Literal(Option<&'a T>),

    /// A string with a language, a direction, or both.
    LangString {
        language: Option<&'a LenientLangTag>,
        direction: Option<Direction>,
    },

    /// A `@json` literal.
    Json,
}

impl<'a, T> ValueObjectFeatures<'a, T> {
    /// Reads the features of an expanded value object.
    fn of_value(value: &'a Value<T>) -> Self {
        match value {
            Value::Literal(_, datatype) => Self::Literal(datatype.as_ref()),
            Value::LangString(s) => Self::LangString {
                language: s.language(),
                direction: s.direction(),
            },
            Value::Json(_) => Self::Json,
        }
    }

    /// The value's `@language`.
    pub(crate) fn language(&self) -> Option<&'a LenientLangTag> {
        match self {
            Self::LangString { language, .. } => *language,
            Self::Literal(_) | Self::Json => None,
        }
    }

    /// The value's `@direction`.
    pub(crate) fn direction(&self) -> Option<Direction> {
        match self {
            Self::LangString { direction, .. } => *direction,
            Self::Literal(_) | Self::Json => None,
        }
    }

    /// The value's `@type`, with `@json` literals reporting [`Type::Json`].
    pub(crate) fn typ(&self) -> Option<Type<&'a T>> {
        match self {
            Self::Literal(Some(datatype)) => Some(Type::Iri(*datatype)),
            Self::Json => Some(Type::Json),
            Self::Literal(None) | Self::LangString { .. } => None,
        }
    }
}

/// Values IRI compaction can read [`ValueFeatures`] from.
pub(crate) trait IntoValueFeatures<'a, T, B> {
    /// Derives the features of this value under `active_context`.
    fn into_value_features(self, active_context: &'a Context<T, B>) -> ValueFeatures<'a, T, B>;
}

impl<'a, T: Clone + PartialEq, B, O: object::Any<T, B>> IntoValueFeatures<'a, T, B> for &'a Indexed<O> {
    fn into_value_features(self, active_context: &'a Context<T, B>) -> ValueFeatures<'a, T, B> {
        let shape = match self.inner().as_ref() {
            object::Ref::List(list) => list_shape(
                list.iter().map(|item| match item.inner() {
                    Object::Value(value) => Some(ValueObjectFeatures::of_value(value)),
                    Object::Node(_) | Object::List(_) => None,
                }),
                list.is_empty(),
                active_context,
            ),
            object::Ref::Node(node) if node.is_graph() => ValueShape::Graph { id: node.id.as_ref() },
            object::Ref::Node(node) => ValueShape::Node { id: node.id.as_ref() },
            object::Ref::Value(value) => ValueShape::Value(ValueObjectFeatures::of_value(value)),
        };

        ValueFeatures {
            has_index: self.index().is_some(),
            shape,
        }
    }
}

impl<'a, T: Clone + PartialEq, B> IntoValueFeatures<'a, T, B> for ValueHint<'a, T, B> {
    fn into_value_features(self, active_context: &'a Context<T, B>) -> ValueFeatures<'a, T, B> {
        let shape = match self.kind {
            ValueKind::Node { id } => ValueShape::Node { id },
            ValueKind::EmptyArray => ValueShape::Node { id: None },
            ValueKind::Graph { id } => ValueShape::Graph { id },
            ValueKind::Literal { datatype } => ValueShape::Value(ValueObjectFeatures::Literal(datatype)),
            ValueKind::LangString { language, direction } => ValueShape::Value(ValueObjectFeatures::LangString { language, direction }),
            ValueKind::Json => ValueShape::Value(ValueObjectFeatures::Json),
            ValueKind::List(items) => list_shape(items.iter().map(|item| item.value_object()), items.is_empty(), active_context),
        };

        ValueFeatures {
            has_index: self.indexed,
            shape,
        }
    }
}

/// Reduces the items of a list object to the type, or the language and
/// direction, they have in common, following step 2.6 of the
/// [IRI compaction algorithm][1].
///
/// `items` yields the features of each item that is a value object and `None`
/// for every other item. An empty list takes the default language and base
/// direction of `active_context`.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#iri-compaction
fn list_shape<'a, T: Clone + PartialEq, B>(
    items: impl Iterator<Item = Option<ValueObjectFeatures<'a, T>>>,
    is_empty: bool,
    active_context: &'a Context<T, B>,
) -> ValueShape<'a, T, B> {
    let mut common_type = None;
    let mut common_lang_dir = None;

    if is_empty {
        common_lang_dir = Some(Nullable::Some((active_context.default_language(), active_context.default_base_direction())));
    } else {
        for item in items {
            let mut item_type = None;
            let mut item_lang_dir = None;
            let is_value = item.is_some();

            match item {
                Some(ValueObjectFeatures::LangString { language, direction }) => item_lang_dir = Some(Nullable::Some((language, direction))),
                Some(ValueObjectFeatures::Literal(Some(datatype))) => item_type = Some(Type::Iri(datatype.clone())),
                Some(ValueObjectFeatures::Literal(None)) => item_lang_dir = Some(Nullable::Null),
                Some(ValueObjectFeatures::Json) => item_type = Some(Type::Json),
                None => item_type = Some(Type::Id),
            }

            if common_lang_dir.is_none() {
                common_lang_dir = item_lang_dir;
            } else if is_value && common_lang_dir != item_lang_dir {
                common_lang_dir = Some(Nullable::Some((None, None)));
            }

            if common_type.is_none() {
                common_type = Some(item_type);
            } else if common_type.as_ref().is_some_and(|t| *t != item_type) {
                common_type = Some(None);
            }

            if common_lang_dir == Some(Nullable::Some((None, None))) && common_type == Some(None) {
                break;
            }
        }
    }

    ValueShape::List {
        common_type: common_type.unwrap_or(None),
        common_lang_dir: common_lang_dir.unwrap_or(Nullable::Some((None, None))),
        is_empty,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::{
        Options,
        compact_iri_with,
        compact_iri_with_hint,
        test_fixtures::{TestContext, TestObject, expanded_object, iri, iri_term, processed_context},
    };
    use jsonld_core::{Node, ProcessingMode};
    use rdfx::{BlankIdBuf, IriBuf, vocabulary::no_vocabulary};
    use std::collections::BTreeSet;

    /// Defines a term for `https://example.com/name` with every type, language
    /// and direction mapping term selection distinguishes, and no container
    /// mapping, so that the type and language preference decide the term.
    const MAPPING_CONTEXT: &str = r#"{
        "@vocab": "https://vocab.example/",
        "ex": "https://example.com/",
        "xsd": "http://www.w3.org/2001/XMLSchema#",
        "Building": "https://example.com/Building",
        "name": "https://example.com/name",
        "nameEn": { "@id": "https://example.com/name", "@language": "en" },
        "nameArRtl": { "@id": "https://example.com/name", "@language": "ar", "@direction": "rtl" },
        "nameLtr": { "@id": "https://example.com/name", "@direction": "ltr" },
        "nameCount": { "@id": "https://example.com/name", "@type": "xsd:integer" },
        "nameRef": { "@id": "https://example.com/name", "@type": "@id" },
        "nameVocab": { "@id": "https://example.com/name", "@type": "@vocab" },
        "nameJson": { "@id": "https://example.com/name", "@type": "@json" },
        "nameReverse": { "@reverse": "https://example.com/name" }
    }"#;

    /// [`MAPPING_CONTEXT`] plus a term for `https://example.com/name` with
    /// every container mapping term selection distinguishes, so that the
    /// container preference decides the term.
    const SELECTION_CONTEXT: &str = r#"{
        "@vocab": "https://vocab.example/",
        "ex": "https://example.com/",
        "xsd": "http://www.w3.org/2001/XMLSchema#",
        "Building": "https://example.com/Building",
        "name": "https://example.com/name",
        "nameEn": { "@id": "https://example.com/name", "@language": "en" },
        "nameArRtl": { "@id": "https://example.com/name", "@language": "ar", "@direction": "rtl" },
        "nameLtr": { "@id": "https://example.com/name", "@direction": "ltr" },
        "nameCount": { "@id": "https://example.com/name", "@type": "xsd:integer" },
        "nameRef": { "@id": "https://example.com/name", "@type": "@id" },
        "nameVocab": { "@id": "https://example.com/name", "@type": "@vocab" },
        "nameJson": { "@id": "https://example.com/name", "@type": "@json" },
        "nameList": { "@id": "https://example.com/name", "@container": "@list" },
        "nameSet": { "@id": "https://example.com/name", "@container": "@set" },
        "nameMap": { "@id": "https://example.com/name", "@container": "@language" },
        "nameIndex": { "@id": "https://example.com/name", "@container": "@index" },
        "nameById": { "@id": "https://example.com/name", "@container": "@id" },
        "nameGraph": { "@id": "https://example.com/name", "@container": "@graph" },
        "nameNamedGraphs": { "@id": "https://example.com/name", "@container": ["@graph", "@id"] },
        "nameReverse": { "@reverse": "https://example.com/name" }
    }"#;

    /// An IRI with many candidate terms, one only reachable as a compact IRI,
    /// and one only reachable through `@vocab`.
    const PROPERTIES: [&str; 3] = ["https://example.com/name", "https://example.com/unlisted", "https://vocab.example/temperature"];

    const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

    /// Asserts that `hint` selects the term `object` selects, for every
    /// property of [`PROPERTIES`], forward and reverse, under both processing
    /// modes, and returns the term selected for the forward
    /// `https://example.com/name` property under JSON-LD 1.1.
    fn assert_same_selection(context: &TestContext, object: &TestObject, hint: ValueHint<IriBuf, BlankIdBuf>) -> Option<String> {
        let modes = [ProcessingMode::JsonLd1_1, ProcessingMode::JsonLd1_0];
        for processing_mode in modes {
            let options = Options {
                processing_mode,
                ..Options::default()
            };
            for property in PROPERTIES {
                let var = iri_term(property);
                for reverse in [false, true] {
                    let from_object = compact_iri_with(no_vocabulary(), context, &var, object, true, reverse, options).map_err(|_| ());
                    let from_hint = compact_iri_with_hint(no_vocabulary(), context, &var, hint, true, reverse, options).map_err(|_| ());
                    assert_eq!(
                        from_object, from_hint,
                        "{object:?} and {hint:?} select different terms for {property} (reverse: {reverse}, {processing_mode:?})"
                    );
                }
            }
        }

        compact_iri_with_hint(no_vocabulary(), context, &iri_term(PROPERTIES[0]), hint, true, false, Options::default())
            .unwrap()
            .map(|term| term.to_string())
    }

    #[tokio::test]
    async fn hint_selects_the_term_its_object_selects() {
        let contexts = [processed_context(MAPPING_CONTEXT).await, processed_context(SELECTION_CONTEXT).await];

        let building = Id::iri(iri("https://example.com/Building"));
        let thing = Id::iri(iri("urn:ngsi-ld:Thing:1"));
        let blank = Id::blank(BlankIdBuf::new("_:b0".to_owned()).unwrap());
        let graph = Id::iri(iri("urn:graph"));
        let xsd_integer = iri(XSD_INTEGER);
        let xsd_date = iri("http://www.w3.org/2001/XMLSchema#date");
        let en = LenientLangTag::new("en").0;
        let ar = LenientLangTag::new("ar").0;

        let plain = ValueKind::Literal { datatype: None };
        let integer = ValueKind::Literal { datatype: Some(&xsd_integer) };
        let english = ValueKind::LangString {
            language: Some(en),
            direction: None,
        };
        let plain_items = [plain, plain];
        let english_items = [english, english];
        let mixed_items = [integer, english, plain];
        let reference_items = [ValueKind::Node { id: Some(&thing) }, ValueKind::Node { id: Some(&building) }];
        let integer_items = [integer, integer];
        let json_items = [ValueKind::Json];

        let cases: Vec<(&str, ValueHint<IriBuf, BlankIdBuf>)> = vec![
            (
                r#"{ "@id": "https://example.com/Building" }"#,
                ValueHint::new(ValueKind::Node { id: Some(&building) }),
            ),
            (r#"{ "@id": "urn:ngsi-ld:Thing:1" }"#, ValueHint::new(ValueKind::Node { id: Some(&thing) })),
            (r#"{ "@id": "_:b0" }"#, ValueHint::new(ValueKind::Node { id: Some(&blank) })),
            (
                r#"{ "@id": "urn:ngsi-ld:Thing:1", "@index": "i" }"#,
                ValueHint::indexed(ValueKind::Node { id: Some(&thing) }),
            ),
            (
                r#"{ "https://example.com/p": [{ "@value": "x" }] }"#,
                ValueHint::new(ValueKind::Node { id: None }),
            ),
            (
                r#"{ "@id": "urn:graph", "@graph": [{ "@id": "urn:ngsi-ld:Thing:1" }] }"#,
                ValueHint::new(ValueKind::Graph { id: Some(&graph) }),
            ),
            (
                r#"{ "@graph": [{ "@id": "urn:ngsi-ld:Thing:1" }], "@index": "i" }"#,
                ValueHint::indexed(ValueKind::Graph { id: None }),
            ),
            (
                r#"{ "@value": "5", "@type": "http://www.w3.org/2001/XMLSchema#integer" }"#,
                ValueHint::new(integer),
            ),
            (
                r#"{ "@value": "2024-01-01", "@type": "http://www.w3.org/2001/XMLSchema#date" }"#,
                ValueHint::new(ValueKind::Literal { datatype: Some(&xsd_date) }),
            ),
            (r#"{ "@value": "x" }"#, ValueHint::new(plain)),
            (r#"{ "@value": 5 }"#, ValueHint::new(plain)),
            (r#"{ "@value": true }"#, ValueHint::new(plain)),
            (r#"{ "@value": "x", "@index": "i" }"#, ValueHint::indexed(plain)),
            (r#"{ "@value": "x", "@language": "en" }"#, ValueHint::new(english)),
            (
                r#"{ "@value": "x", "@language": "ar", "@direction": "rtl" }"#,
                ValueHint::new(ValueKind::LangString {
                    language: Some(ar),
                    direction: Some(Direction::Rtl),
                }),
            ),
            (
                r#"{ "@value": "x", "@direction": "ltr" }"#,
                ValueHint::new(ValueKind::LangString {
                    language: None,
                    direction: Some(Direction::Ltr),
                }),
            ),
            (r#"{ "@value": "x", "@language": "en", "@index": "i" }"#, ValueHint::indexed(english)),
            (r#"{ "@value": { "a": [1, 2.50] }, "@type": "@json" }"#, ValueHint::new(ValueKind::Json)),
            (
                r#"{ "@list": [{ "@value": "a" }, { "@value": "b" }] }"#,
                ValueHint::new(ValueKind::List(&plain_items)),
            ),
            (
                r#"{ "@list": [{ "@value": "a", "@language": "en" }, { "@value": "b", "@language": "en" }] }"#,
                ValueHint::new(ValueKind::List(&english_items)),
            ),
            (
                r#"{ "@list": [{ "@value": "5", "@type": "http://www.w3.org/2001/XMLSchema#integer" }, { "@value": "a", "@language": "en" }, { "@value": "b" }] }"#,
                ValueHint::new(ValueKind::List(&mixed_items)),
            ),
            (
                r#"{ "@list": [{ "@id": "urn:ngsi-ld:Thing:1" }, { "@id": "https://example.com/Building" }] }"#,
                ValueHint::new(ValueKind::List(&reference_items)),
            ),
            (
                r#"{ "@list": [{ "@value": "1", "@type": "http://www.w3.org/2001/XMLSchema#integer" }, { "@value": "2", "@type": "http://www.w3.org/2001/XMLSchema#integer" }] }"#,
                ValueHint::new(ValueKind::List(&integer_items)),
            ),
            (
                r#"{ "@list": [{ "@value": { "a": 1 }, "@type": "@json" }] }"#,
                ValueHint::new(ValueKind::List(&json_items)),
            ),
            (r#"{ "@list": [] }"#, ValueHint::new(ValueKind::List(&[]))),
            (
                r#"{ "@list": [{ "@value": "a" }], "@index": "i" }"#,
                ValueHint::indexed(ValueKind::List(&plain_items[..1])),
            ),
        ];

        let mut selected = BTreeSet::new();
        for context in &contexts {
            for (source, hint) in &cases {
                selected.extend(assert_same_selection(context, &expanded_object(source), *hint));
            }
        }

        // The comparisons above are only meaningful if the values are told
        // apart: most of the contexts' terms must have been selected.
        assert!(selected.len() >= 14, "too few distinct terms selected: {selected:?}");
    }

    /// The empty array of a property without values selects what an `@id`-less
    /// node object selects, which is how the compaction algorithm compacts it.
    #[tokio::test]
    async fn empty_array_hint_selects_the_term_of_an_empty_property() {
        let context = processed_context(SELECTION_CONTEXT).await;
        let empty_node = Indexed::new(Object::node(Node::new()), None);
        let term = assert_same_selection(&context, &empty_node, ValueHint::new(ValueKind::EmptyArray));
        assert_eq!(term.as_deref(), Some("nameById"));
    }
}
