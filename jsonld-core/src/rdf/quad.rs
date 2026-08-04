use super::{RdfDirection, triples::Value};
use crate::{ExpandedDocument, FlattenedDocument, LdQuads, ValidId};
use rdfx::{
    GeneralizedTriple as Triple,
    LocalGenerator,
    vocabulary::{BlankIdVocabulary, IriVocabulary, LiteralVocabulary, Vocabulary, VocabularyMut},
};
use std::{borrow::Cow, convert::TryInto, hash::Hash};

/// RDF quad produced from an expanded document.
pub type Quad<T, B, L> = rdfx::GeneralizedQuad<ValidId<T, B>, ValidId<T, B>, Value<T, B, L>, ValidId<T, B>>;

/// RDF quad whose terms are borrowed from the document.
pub type QuadRef<'a, T, B, L> = rdfx::GeneralizedQuad<Cow<'a, ValidId<T, B>>, Cow<'a, ValidId<T, B>>, Value<T, B, L>, &'a ValidId<T, B>>;

struct Compound<'a, T, B, L> {
    graph: Option<&'a ValidId<T, B>>,
    triples: super::CompoundValueTriples<'a, T, B, L>,
}

type VocabularyCompoundLiteral<'a, N> = Compound<'a, <N as IriVocabulary>::Iri, <N as BlankIdVocabulary>::BlankId, <N as LiteralVocabulary>::Literal>;

/// Warning raised while serializing a document as RDF quads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    /// A value could not be represented as an RDF term; the quad that would
    /// have carried it was dropped from the stream.
    ValueSerializationFailed,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValueSerializationFailed => f.write_str("value not representable as an RDF term, quad dropped"),
        }
    }
}

/// Iterator over the RDF Quads of a JSON-LD document.
pub struct Quads<'a, N: Vocabulary, G: LocalGenerator, W = ()> {
    vocabulary: &'a mut N,
    generator: &'a mut G,
    rdf_direction: Option<RdfDirection>,
    compound_value: Option<VocabularyCompoundLiteral<'a, N>>,
    quads: crate::quad::Quads<'a, N::Iri, N::BlankId>,
    produce_generalized_rdf: bool,
    warnings: W,
}

impl<'a, N: Vocabulary, G: LocalGenerator, W> Quads<'a, N, G, W> {
    /// Turns this iterator into one yielding owned quads.
    pub fn cloned(self) -> ClonedQuads<'a, N, G, W> {
        ClonedQuads { inner: self }
    }

    /// Attaches a warning handler to this iterator, replacing the current
    /// one (`()` initially, which ignores warnings).
    ///
    /// The handler is called whenever a quad is dropped from the stream
    /// because its value has no RDF representation.
    pub fn with_warnings<V>(self, warnings: V) -> Quads<'a, N, G, V> {
        Quads {
            vocabulary: self.vocabulary,
            generator: self.generator,
            rdf_direction: self.rdf_direction,
            compound_value: self.compound_value,
            quads: self.quads,
            produce_generalized_rdf: self.produce_generalized_rdf,
            warnings,
        }
    }
}

impl<'a, N: Vocabulary + VocabularyMut, G: LocalGenerator, W> Iterator for Quads<'a, N, G, W>
where
    N::Iri: Clone,
    N::BlankId: Clone,
    N::Literal: Clone,
    W: crate::warning::Handler<N, Warning>,
{
    type Item = QuadRef<'a, N::Iri, N::BlankId, N::Literal>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(compound_value) = &mut self.compound_value {
                match compound_value.triples.next(self.vocabulary, self.generator, self.rdf_direction) {
                    Some(Triple(subject, property, object)) => {
                        if self.produce_generalized_rdf || !property.is_blank() {
                            break Some(rdfx::GeneralizedQuad(Cow::Owned(subject), Cow::Owned(property), object, compound_value.graph));
                        }
                    }
                    None => self.compound_value = None,
                }
            }

            match self.quads.next() {
                Some(crate::quad::QuadRef(graph, subject, property, object)) => {
                    let rdf_graph: Option<&'a ValidId<N::Iri, N::BlankId>> = match graph.map(|r| r.try_into()) {
                        Some(Ok(r)) => Some(r),
                        None => None,
                        _ => continue,
                    };

                    let rdf_subject: &'a ValidId<N::Iri, N::BlankId> = match subject.try_into() {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    let rdf_property: Cow<ValidId<N::Iri, N::BlankId>> = match property {
                        crate::quad::PropertyRef::Type => Cow::Owned(ValidId::Iri(self.vocabulary.insert(super::RDF_TYPE))),
                        crate::quad::PropertyRef::Ref(r) => match r.try_into() {
                            Ok(r) => Cow::Borrowed(r),
                            Err(_) => continue,
                        },
                    };

                    if !self.produce_generalized_rdf && (*rdf_property).is_blank() {
                        // Skip gRDF quad.
                        continue;
                    }

                    match object.rdf_value_with(self.vocabulary, self.generator, self.rdf_direction) {
                        Some(compound_value) => {
                            if let Some(rdf_value_triples) = compound_value.triples {
                                self.compound_value = Some(Compound {
                                    graph: rdf_graph,
                                    triples: rdf_value_triples,
                                });
                            }

                            break Some(rdfx::GeneralizedQuad(Cow::Borrowed(rdf_subject), rdf_property, compound_value.value, rdf_graph));
                        }
                        None => {
                            self.warnings.handle(self.vocabulary, Warning::ValueSerializationFailed);
                        }
                    }
                }
                None => break None,
            }
        }
    }
}

/// Iterator over the RDF Quads of a JSON-LD document where borrowed values are
/// cloned.
pub struct ClonedQuads<'a, N: Vocabulary, G: LocalGenerator, W = ()> {
    inner: Quads<'a, N, G, W>,
}

impl<'a, N: Vocabulary + VocabularyMut, G: LocalGenerator, W> Iterator for ClonedQuads<'a, N, G, W>
where
    N::Iri: Clone,
    N::BlankId: Clone,
    N::Literal: Clone,
    W: crate::warning::Handler<N, Warning>,
{
    type Item = Quad<N::Iri, N::BlankId, N::Literal>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|rdfx::GeneralizedQuad(s, p, o, g)| rdfx::GeneralizedQuad(s.into_owned(), p.into_owned(), o, g.cloned()))
    }
}

/// Documents that can be serialized as RDF quads.
pub trait RdfQuads<T, B> {
    /// Returns the RDF quads of this document, taking every parameter
    /// explicitly.
    fn rdf_quads_full<'a, V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &'a self,
        vocabulary: &'a mut V,
        generator: &'a mut G,
        rdf_direction: Option<RdfDirection>,
        produce_generalized_rdf: bool,
    ) -> Quads<'a, V, G>;

    /// Returns the RDF quads of this document, using the given vocabulary.
    fn rdf_quads_with<'a, V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &'a self,
        vocabulary: &'a mut V,
        generator: &'a mut G,
        rdf_direction: Option<RdfDirection>,
    ) -> Quads<'a, V, G> {
        self.rdf_quads_full(vocabulary, generator, rdf_direction, false)
    }

    /// Returns the RDF quads of this document.
    fn rdf_quads<'a, G: LocalGenerator>(&'a self, generator: &'a mut G, rdf_direction: Option<RdfDirection>) -> Quads<'a, (), G>
    where
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.rdf_quads_with(rdfx::vocabulary::no_vocabulary_mut(), generator, rdf_direction)
    }
}

impl<T, B> RdfQuads<T, B> for ExpandedDocument<T, B> {
    fn rdf_quads_full<'a, V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &'a self,
        vocabulary: &'a mut V,
        generator: &'a mut G,
        rdf_direction: Option<RdfDirection>,
        produce_generalized_rdf: bool,
    ) -> Quads<'a, V, G> {
        Quads {
            vocabulary,
            generator,
            rdf_direction,
            compound_value: None,
            quads: self.quads(),
            produce_generalized_rdf,
            warnings: (),
        }
    }
}

impl<T, B> RdfQuads<T, B> for FlattenedDocument<T, B> {
    fn rdf_quads_full<'a, V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &'a self,
        vocabulary: &'a mut V,
        generator: &'a mut G,
        rdf_direction: Option<RdfDirection>,
        produce_generalized_rdf: bool,
    ) -> Quads<'a, V, G> {
        Quads {
            vocabulary,
            generator,
            rdf_direction,
            compound_value: None,
            quads: self.quads(),
            produce_generalized_rdf,
            warnings: (),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> RdfQuads<T, B> for crate::flattening::NodeMap<T, B> {
    fn rdf_quads_full<'a, V: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &'a self,
        vocabulary: &'a mut V,
        generator: &'a mut G,
        rdf_direction: Option<RdfDirection>,
        produce_generalized_rdf: bool,
    ) -> Quads<'a, V, G> {
        Quads {
            vocabulary,
            generator,
            rdf_direction,
            compound_value: None,
            quads: self.quads(),
            produce_generalized_rdf,
            warnings: (),
        }
    }
}
