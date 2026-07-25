//! RDF interop helpers and well-known IRI constants.
//!
//! The full RDF triple/quad machinery (Generator-driven, Vocabulary-aware) lives
//! in `triples.rs` and is gated until task 8 ports it to rdfx's
//! `GeneralizedTriple`/`LocalGenerator` API.
use iri_rs::{Iri, iri};

/// RDF quads produced from an expanded document.
pub mod quad;
mod triples;

pub use quad::*;
pub use triples::*;

/// The `rdf:type` IRI.
pub const RDF_TYPE: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
/// The `rdf:first` IRI, holding the head of a list.
pub const RDF_FIRST: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#first");
/// The `rdf:rest` IRI, holding the tail of a list.
pub const RDF_REST: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");
/// The `rdf:value` IRI.
pub const RDF_VALUE: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#value");
/// The `rdf:direction` IRI, carrying the base direction of a string.
pub const RDF_DIRECTION: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#direction");
/// The `rdf:JSON` datatype IRI.
pub const RDF_JSON: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON");
/// IRI of the `http://www.w3.org/1999/02/22-rdf-syntax-ns#nil` value.
pub const RDF_NIL: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");

/// The `xsd:boolean` datatype IRI.
pub const XSD_BOOLEAN: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#boolean");
/// The `xsd:integer` datatype IRI.
pub const XSD_INTEGER: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#integer");
/// The `xsd:double` datatype IRI.
pub const XSD_DOUBLE: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#double");
/// The `xsd:string` datatype IRI.
pub const XSD_STRING: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#string");

/// Direction representation method.
///
/// Used by the RDF serializer to decide how to encode
/// [`Direction`](crate::Direction)s.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RdfDirection {
    /// Encode direction in the string value type IRI using the
    /// `https://www.w3.org/ns/i18n#` prefix.
    I18nDatatype,

    /// Encode the direction using a compound literal value.
    CompoundLiteral,
}

#[derive(Debug, Clone)]
/// Error raised when a string is not a valid base direction.
pub struct InvalidRdfDirection(pub String);

impl std::str::FromStr for RdfDirection {
    type Err = InvalidRdfDirection;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "i18n-datatype" => Ok(Self::I18nDatatype),
            "compound-literal" => Ok(Self::CompoundLiteral),
            _ => Err(InvalidRdfDirection(s.to_string())),
        }
    }
}

impl<'a> TryFrom<&'a str> for RdfDirection {
    type Error = InvalidRdfDirection;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        value.parse()
    }
}
