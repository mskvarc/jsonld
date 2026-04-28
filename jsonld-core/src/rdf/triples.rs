use super::{RDF_DIRECTION, RDF_FIRST, RDF_JSON, RDF_NIL, RDF_REST, RDF_VALUE, RdfDirection, XSD_BOOLEAN, XSD_DOUBLE, XSD_INTEGER, XSD_STRING};
use crate::{Direction, Id, Indexed, IndexedObject, Node, Object, ValidId, object::value};
use iri_rs::IriBuf;
use json_syntax::Print;
use langtag::LangTagBuf;
use rdf_rs::{
    Literal,
    LocalGenerator,
    vocabulary::{IriVocabularyMut, Vocabulary, VocabularyMut},
};
use smallvec::SmallVec;

/// JSON-LD to RDF triple.
pub type Triple<T, B, L> = rdf_rs::GeneralizedTriple<ValidId<T, B>, ValidId<T, B>, Value<T, B, L>>;

/// Build a [`rdf_rs::Datatype`] from a static IRI constant. The IRIs used by
/// jsonld-core are never `rdf:langString` / `rdf:dirLangString`, so we fall
/// back to [`Datatype::new_unchecked`] only as a defensive last resort.
fn static_datatype(iri: iri_rs::Iri<&'static str>) -> rdf_rs::Datatype {
    rdf_rs::Datatype::new(IriBuf::from(iri)).unwrap_or_else(|e| unsafe { rdf_rs::Datatype::new_unchecked(e.into_iri()) })
}

/// Build a [`rdf_rs::Datatype`] from a vocabulary handle.
fn datatype_of<V: rdf_rs::vocabulary::IriVocabulary>(vocabulary: &V, iri: &V::Iri) -> rdf_rs::Datatype {
    let buf = IriBuf::from(vocabulary.iri(iri).unwrap());
    rdf_rs::Datatype::new(buf).unwrap_or_else(|e| unsafe { rdf_rs::Datatype::new_unchecked(e.into_iri()) })
}

impl<T: Clone, B: Clone> Id<T, B> {
    fn rdf_value<L>(&self) -> Option<Value<T, B, L>> {
        match self {
            Id::Valid(id) => Some(Value::Id(id.clone())),
            Id::Invalid(_) => None,
        }
    }
}

/// Iterator over the triples of a compound literal representing a language
/// tagged string with direction.
pub struct CompoundLiteralTriples<T, B, L> {
    /// Compound literal identifier.
    id: ValidId<T, B>,

    /// String value.
    value: Option<Value<T, B, L>>,

    /// Direction value.
    direction: Option<Value<T, B, L>>,
}

impl<T: Clone, B: Clone, L: Clone> CompoundLiteralTriples<T, B, L> {
    fn next(&mut self, vocabulary: &mut impl IriVocabularyMut<Iri = T>) -> Option<Triple<T, B, L>> {
        if let Some(value) = self.value.take() {
            return Some(rdf_rs::GeneralizedTriple(self.id.clone(), ValidId::Iri(vocabulary.insert(RDF_VALUE)), value));
        }

        if let Some(direction) = self.direction.take() {
            return Some(rdf_rs::GeneralizedTriple(
                self.id.clone(),
                ValidId::Iri(vocabulary.insert(RDF_DIRECTION)),
                direction,
            ));
        }

        None
    }
}

/// Compound literal.
pub struct CompoundLiteral<T, B, L> {
    value: Value<T, B, L>,
    triples: Option<CompoundLiteralTriples<T, B, L>>,
}

impl<T: Clone> crate::object::Value<T> {
    fn rdf_value_with<V, G: LocalGenerator>(
        &self,
        vocabulary: &mut V,
        generator: &mut G,
        rdf_direction: Option<RdfDirection>,
    ) -> Option<CompoundLiteral<T, V::BlankId, V::Literal>>
    where
        V: Vocabulary<Iri = T> + VocabularyMut,
    {
        match self {
            Self::Json(json) => Some(CompoundLiteral {
                value: Value::Literal(vocabulary.insert_owned_literal(Literal::new(
                    json.compact_print().to_string(),
                    rdf_rs::LiteralType::Any(static_datatype(RDF_JSON)),
                ))),
                triples: None,
            }),
            Self::LangString(lang_string) => {
                let (string, language, direction) = lang_string.parts();

                let language = match language {
                    Some(language) => match language.as_well_formed() {
                        Some(tag) => Some(tag.to_owned()),
                        None => return None,
                    },
                    None => None,
                };

                match direction {
                    Some(direction) => match rdf_direction {
                        Some(RdfDirection::I18nDatatype) => {
                            let ty =
                                rdf_rs::Datatype::new(i18n(language, *direction)).unwrap_or_else(|e| unsafe { rdf_rs::Datatype::new_unchecked(e.into_iri()) });
                            Some(CompoundLiteral {
                                value: Value::Literal(vocabulary.insert_owned_literal(Literal::new(string.to_string(), rdf_rs::LiteralType::Any(ty)))),
                                triples: None,
                            })
                        }
                        Some(RdfDirection::CompoundLiteral) => {
                            let id = crate::id::generator_next_id(vocabulary, generator);
                            Some(CompoundLiteral {
                                value: Value::from_id(id),
                                triples: None,
                            })
                        }
                        None => match language {
                            Some(tag) => Some(CompoundLiteral {
                                value: Value::Literal(vocabulary.insert_owned_literal(Literal::new(string.to_string(), rdf_rs::LiteralType::LangString(tag)))),
                                triples: None,
                            }),
                            None => Some(CompoundLiteral {
                                value: Value::Literal(
                                    vocabulary.insert_owned_literal(Literal::new(string.to_string(), rdf_rs::LiteralType::Any(static_datatype(XSD_STRING)))),
                                ),
                                triples: None,
                            }),
                        },
                    },
                    None => match language {
                        Some(tag) => Some(CompoundLiteral {
                            value: Value::Literal(vocabulary.insert_owned_literal(Literal::new(string.to_string(), rdf_rs::LiteralType::LangString(tag)))),
                            triples: None,
                        }),
                        None => Some(CompoundLiteral {
                            value: Value::Literal(
                                vocabulary.insert_owned_literal(Literal::new(string.to_string(), rdf_rs::LiteralType::Any(static_datatype(XSD_STRING)))),
                            ),
                            triples: None,
                        }),
                    },
                }
            }
            Self::Literal(lit, ty) => {
                let (rdf_lit, preferred_rdf_ty) = match lit {
                    value::Literal::Boolean(b) => {
                        let lit = if *b { "true".to_string() } else { "false".to_string() };

                        (lit, Some(static_datatype(XSD_BOOLEAN)))
                    }
                    value::Literal::Null => ("null".to_string(), None),
                    value::Literal::Number(n) => {
                        if n.is_i64() && !ty.as_ref().map(|t| vocabulary.iri(t).unwrap() == XSD_DOUBLE).unwrap_or(false) {
                            (n.to_string(), Some(static_datatype(XSD_INTEGER)))
                        } else {
                            (pretty_dtoa::dtoa(n.as_f64_lossy(), XSD_CANONICAL_FLOAT), Some(static_datatype(XSD_DOUBLE)))
                        }
                    }
                    value::Literal::String(s) => (s.to_string(), None),
                };

                let rdf_ty = match ty {
                    Some(id) => Some(datatype_of(vocabulary, id)),
                    None => preferred_rdf_ty,
                };

                Some(CompoundLiteral {
                    value: Value::Literal(vocabulary.insert_owned_literal(Literal::new(
                        rdf_lit,
                        rdf_rs::LiteralType::Any(rdf_ty.unwrap_or_else(|| static_datatype(XSD_STRING))),
                    ))),
                    triples: None,
                })
            }
        }
    }
}

// <https://www.w3.org/TR/xmlschema11-2/#f-doubleLexmap>
const XSD_CANONICAL_FLOAT: pretty_dtoa::FmtFloatConfig = pretty_dtoa::FmtFloatConfig::default().force_e_notation().capitalize_e(true);

impl<T: Clone, B: Clone> Node<T, B> {
    fn rdf_value<L>(&self) -> Option<Value<T, B, L>> {
        self.id.as_ref().and_then(Id::rdf_value)
    }
}

impl<T: Clone, B: Clone> Object<T, B> {
    fn rdf_value_with<V, G: LocalGenerator>(
        &self,
        vocabulary: &mut V,
        generator: &mut G,
        rdf_direction: Option<RdfDirection>,
    ) -> Option<CompoundValue<'_, T, B, V::Literal>>
    where
        V: Vocabulary<Iri = T, BlankId = B> + VocabularyMut,
    {
        match self {
            Self::Value(value) => value.rdf_value_with(vocabulary, generator, rdf_direction).map(|compound_value| CompoundValue {
                value: compound_value.value,
                triples: compound_value.triples.map(CompoundValueTriples::literal),
            }),
            Self::Node(node) => node.rdf_value().map(|value| CompoundValue { value, triples: None }),
            Self::List(list) => {
                if list.is_empty() {
                    Some(CompoundValue {
                        value: Value::Id(ValidId::Iri(vocabulary.insert(RDF_NIL))),
                        triples: None,
                    })
                } else {
                    let id = crate::id::generator_next_id(vocabulary, generator);
                    Some(CompoundValue {
                        value: Value::from_id(id.clone()),
                        triples: Some(CompoundValueTriples::List(ListTriples::new(list.as_slice(), id))),
                    })
                }
            }
        }
    }
}

pub struct CompoundValue<'a, T, B, L> {
    pub value: Value<T, B, L>,
    pub triples: Option<CompoundValueTriples<'a, T, B, L>>,
}

impl<'a, T: Clone, B: Clone> crate::quad::ObjectRef<'a, T, B> {
    pub fn rdf_value_with<V, G: LocalGenerator>(
        &self,
        vocabulary: &mut V,
        generator: &mut G,
        rdf_direction: Option<RdfDirection>,
    ) -> Option<CompoundValue<'a, T, B, V::Literal>>
    where
        V: Vocabulary<Iri = T, BlankId = B> + VocabularyMut,
    {
        match self {
            Self::Object(object) => object.rdf_value_with(vocabulary, generator, rdf_direction),
            Self::Node(node) => node.rdf_value().map(|value| CompoundValue { value, triples: None }),
            Self::Ref(r) => r.rdf_value().map(|value| CompoundValue { value, triples: None }),
        }
    }
}

enum ListItemTriples<'a, T, B, L> {
    NestedList(NestedListTriples<'a, T, B>),
    CompoundLiteral(Box<CompoundLiteralTriples<T, B, L>>),
}

struct NestedListTriples<'a, T, B> {
    head_ref: Option<ValidId<T, B>>,
    previous: Option<ValidId<T, B>>,
    iter: std::slice::Iter<'a, IndexedObject<T, B>>,
}

struct ListNode<'a, 'i, T, B> {
    id: &'i ValidId<T, B>,
    object: &'a Indexed<Object<T, B>>,
}

impl<'a, T, B> NestedListTriples<'a, T, B> {
    fn new(list: &'a [IndexedObject<T, B>], head_ref: ValidId<T, B>) -> Self {
        Self {
            head_ref: Some(head_ref),
            previous: None,
            iter: list.iter(),
        }
    }

    fn previous(&self) -> Option<&ValidId<T, B>> {
        self.previous.as_ref()
    }

    /// Pull the next object of the list.
    ///
    /// Uses the given generator to assign as id to the list element.
    fn next<V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(&mut self, vocabulary: &mut V, generator: &mut G) -> Option<ListNode<'a, '_, T, B>>
    where
        V: VocabularyMut,
    {
        if let Some(next) = self.iter.next() {
            let id = match self.head_ref.take() {
                Some(id) => id,
                None => crate::id::generator_next_id(vocabulary, generator),
            };

            self.previous = Some(id);
            Some(ListNode {
                object: next,
                id: self.previous.as_ref().unwrap(),
            })
        } else {
            None
        }
    }
}

pub enum CompoundValueTriples<'a, T, B, L> {
    Literal(Box<CompoundLiteralTriples<T, B, L>>),
    List(ListTriples<'a, T, B, L>),
}

impl<'a, T, B, L> CompoundValueTriples<'a, T, B, L> {
    pub fn literal(l: CompoundLiteralTriples<T, B, L>) -> Self {
        Self::Literal(Box::new(l))
    }

    pub fn with<'n, V: Vocabulary<Iri = T, BlankId = B, Literal = L>, G: LocalGenerator>(
        self,
        vocabulary: &'n mut V,
        generator: G,
        rdf_direction: Option<RdfDirection>,
    ) -> CompoundValueTriplesWith<'a, 'n, V, G> {
        CompoundValueTriplesWith {
            vocabulary,
            generator,
            rdf_direction,
            inner: self,
        }
    }

    pub fn next<V, G: LocalGenerator>(&mut self, vocabulary: &mut V, generator: &mut G, rdf_direction: Option<RdfDirection>) -> Option<Triple<T, B, L>>
    where
        T: Clone,
        B: Clone,
        L: Clone,
        V: Vocabulary<Iri = T, BlankId = B, Literal = L> + VocabularyMut,
    {
        match self {
            Self::Literal(l) => l.next(vocabulary),
            Self::List(l) => l.next(vocabulary, generator, rdf_direction),
        }
    }
}

pub struct CompoundValueTriplesWith<'a, 'n, N: Vocabulary, G: LocalGenerator> {
    vocabulary: &'n mut N,
    generator: G,
    rdf_direction: Option<RdfDirection>,
    inner: CompoundValueTriples<'a, N::Iri, N::BlankId, N::Literal>,
}

impl<'a, 'n, N: Vocabulary + VocabularyMut, G: LocalGenerator> Iterator for CompoundValueTriplesWith<'a, 'n, N, G>
where
    N::Iri: Clone,
    N::BlankId: Clone,
    N::Literal: Clone,
{
    type Item = Triple<N::Iri, N::BlankId, N::Literal>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next(self.vocabulary, &mut self.generator, self.rdf_direction)
    }
}

/// Iterator over the RDF quads generated from a list of JSON-LD objects.
///
/// If the list contains nested lists, the iterator will also emit quads for those nested lists.
pub struct ListTriples<'a, T, B, L> {
    stack: SmallVec<[ListItemTriples<'a, T, B, L>; 2]>,
    pending: Option<Triple<T, B, L>>,
}

impl<'a, T, B, L> ListTriples<'a, T, B, L> {
    pub fn new(list: &'a [IndexedObject<T, B>], head_ref: ValidId<T, B>) -> Self {
        let mut stack = SmallVec::new();
        stack.push(ListItemTriples::NestedList(NestedListTriples::new(list, head_ref)));

        Self { stack, pending: None }
    }

    pub fn with<'n, V: Vocabulary<Iri = T, BlankId = B, Literal = L>, G: LocalGenerator>(
        self,
        vocabulary: &'n mut V,
        generator: G,
        rdf_direction: Option<RdfDirection>,
    ) -> ListTriplesWith<'a, 'n, V, G> {
        ListTriplesWith {
            vocabulary,
            generator,
            rdf_direction,
            inner: self,
        }
    }

    pub fn next<V, G: LocalGenerator>(&mut self, vocabulary: &mut V, generator: &mut G, rdf_direction: Option<RdfDirection>) -> Option<Triple<T, B, L>>
    where
        T: Clone,
        B: Clone,
        L: Clone,
        V: Vocabulary<Iri = T, BlankId = B, Literal = L> + VocabularyMut,
    {
        loop {
            if let Some(pending) = self.pending.take() {
                break Some(pending);
            }

            match self.stack.last_mut() {
                Some(ListItemTriples::CompoundLiteral(lit)) => match lit.next(vocabulary) {
                    Some(triple) => break Some(triple),
                    None => {
                        self.stack.pop();
                    }
                },
                Some(ListItemTriples::NestedList(list)) => {
                    let previous = list.previous().cloned();
                    match list.next(vocabulary, generator) {
                        Some(node) => {
                            if let Some(compound_value) = node.object.rdf_value_with(vocabulary, generator, rdf_direction) {
                                let id = node.id.clone();

                                if let Some(compound_triples) = compound_value.triples {
                                    match compound_triples {
                                        CompoundValueTriples::List(list) => self.stack.extend(list.stack),
                                        CompoundValueTriples::Literal(lit) => self.stack.push(ListItemTriples::CompoundLiteral(lit)),
                                    }
                                }

                                self.pending = Some(rdf_rs::GeneralizedTriple(
                                    id.clone(),
                                    ValidId::Iri(vocabulary.insert(RDF_FIRST)),
                                    compound_value.value,
                                ));

                                if let Some(previous_id) = previous {
                                    break Some(rdf_rs::GeneralizedTriple(
                                        previous_id,
                                        ValidId::Iri(vocabulary.insert(RDF_REST)),
                                        Value::from_id(id),
                                    ));
                                }
                            }
                        }
                        None => {
                            self.stack.pop();
                            if let Some(previous_id) = previous {
                                break Some(rdf_rs::GeneralizedTriple(
                                    previous_id,
                                    ValidId::Iri(vocabulary.insert(RDF_REST)),
                                    Value::Id(ValidId::Iri(vocabulary.insert(RDF_NIL))),
                                ));
                            }
                        }
                    }
                }
                None => break None,
            }
        }
    }
}

pub struct ListTriplesWith<'a, 'n, V: Vocabulary, G: LocalGenerator> {
    vocabulary: &'n mut V,
    generator: G,
    rdf_direction: Option<RdfDirection>,
    inner: ListTriples<'a, V::Iri, V::BlankId, V::Literal>,
}

impl<'a, 'n, N: Vocabulary + VocabularyMut, G: LocalGenerator> Iterator for ListTriplesWith<'a, 'n, N, G>
where
    N::Iri: Clone,
    N::BlankId: Clone,
    N::Literal: Clone,
{
    type Item = Triple<N::Iri, N::BlankId, N::Literal>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next(self.vocabulary, &mut self.generator, self.rdf_direction)
    }
}

fn i18n(language: Option<LangTagBuf>, direction: Direction) -> IriBuf {
    let iri = match &language {
        Some(language) => format!("https://www.w3.org/ns/i18n#{language}_{direction}"),
        None => format!("https://www.w3.org/ns/i18n#{direction}"),
    };

    IriBuf::new(iri).unwrap()
}

/// RDF object value: either a node identifier or a literal handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value<T, B, L> {
    Id(ValidId<T, B>),
    Literal(L),
}

impl<T, B, L> Value<T, B, L> {
    pub fn from_id(id: ValidId<T, B>) -> Self {
        Self::Id(id)
    }
}
