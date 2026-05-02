use contextual::DisplayWithContext;
use iri_rs::IriEnum;
use jsonld::ValidId;
pub use rdf_rs::vocabulary::{BlankIdIndex, IriIndex, LiteralIndex};
use rdf_rs::{
    impl_resource,
    vocabulary::{BlankIdVocabulary, IriVocabulary, LiteralVocabulary},
};
use std::fmt;

/// Quad shape stored by the proc-macro's working dataset.
pub type IndexQuad = rdf_rs::Quad<IndexTerm, IndexTerm, IndexTerm, IndexTerm>;

/// Uniform resource type used for every position of a quad in the
/// proc-macro-internal dataset. `impl_resource!` opts it into all four
/// position-trait sealed impls (subject/predicate/object/graph) so it can
/// satisfy [`IndexedBTreeDataset`]'s `R: Resource` requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexTerm {
    Iri(IriIndex),
    Blank(BlankIdIndex),
    Literal(LiteralIndex),
}

impl_resource!(IndexTerm);

impl IndexTerm {
    pub const fn iri(i: IriIndex) -> Self {
        Self::Iri(i)
    }

    pub fn from_id(id: ValidId<IriIndex, BlankIdIndex>) -> Self {
        match id {
            ValidId::Iri(i) => Self::Iri(i),
            ValidId::Blank(b) => Self::Blank(b),
        }
    }
}

impl<V> DisplayWithContext<V> for IndexTerm
where
    V: IriVocabulary<Iri = IriIndex> + BlankIdVocabulary<BlankId = BlankIdIndex> + LiteralVocabulary<Literal = LiteralIndex>,
{
    fn fmt_with(&self, vocabulary: &V, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iri(i) => write!(f, "{}", vocabulary.iri(i).unwrap()),
            Self::Blank(b) => write!(f, "{}", vocabulary.blank_id(b).unwrap()),
            Self::Literal(l) => {
                let lit = vocabulary.literal(l).unwrap();
                write!(f, "{:?}", lit.value)
            }
        }
    }
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Vocab {
    Rdf(Rdf),
    Rdfs(Rdfs),
    Xsd(Xsd),
    Manifest(Manifest),
    Test(Test),
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[iri_prefix("rdf" = "http://www.w3.org/1999/02/22-rdf-syntax-ns#")]
pub enum Rdf {
    #[iri("rdf:type")]
    Type,
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[iri_prefix("rdfs" = "http://www.w3.org/2000/01/rdf-schema#")]
pub enum Rdfs {
    #[iri("rdfs:comment")]
    Comment,
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[iri_prefix("xsd" = "http://www.w3.org/2001/XMLSchema#")]
pub enum Xsd {
    #[iri("xsd:boolean")]
    Boolean,

    #[iri("xsd:string")]
    String,
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[iri_prefix("manifest" = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#")]
pub enum Manifest {
    #[iri("manifest:name")]
    Name,
    #[iri("manifest:entries")]
    Entries,
    #[iri("manifest:action")]
    Action,
    #[iri("manifest:result")]
    Result,
}

#[derive(Debug, IriEnum, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[iri_prefix("test" = "https://w3c.github.io/json-ld-api/tests/vocab#")]
pub enum Test {
    #[iri("test:PositiveEvaluationTest")]
    PositiveEval,
    #[iri("test:NegativeEvaluationTest")]
    NegativeEval,
    #[iri("test:context")]
    Context,
    #[iri("test:option")]
    Option,
    #[iri("test:base")]
    Base,
    #[iri("test:compactArrays")]
    CompactArrays,
    #[iri("test:processingMode")]
    ProcessingMode,
    #[iri("test:specVersion")]
    SpecVersion,
}
