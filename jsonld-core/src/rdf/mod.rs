//! RDF interop helpers and well-known IRI constants.
//!
//! The full RDF triple/quad machinery (Generator-driven, Vocabulary-aware) lives
//! in `triples.rs` and is gated until task 8 ports it to rdf-rs's
//! `GeneralizedTriple`/`LocalGenerator` API.
use iri_rs::{Iri, iri};

pub mod quad;
mod triples;

pub use quad::*;
pub use triples::*;

pub const RDF_TYPE: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
pub const RDF_FIRST: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#first");
pub const RDF_REST: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");
pub const RDF_VALUE: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#value");
pub const RDF_DIRECTION: Iri<&'static str> =
	iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#direction");
pub const RDF_JSON: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON");
/// IRI of the `http://www.w3.org/1999/02/22-rdf-syntax-ns#nil` value.
pub const RDF_NIL: Iri<&'static str> = iri!("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");

pub const XSD_BOOLEAN: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#boolean");
pub const XSD_INTEGER: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#integer");
pub const XSD_DOUBLE: Iri<&'static str> = iri!("http://www.w3.org/2001/XMLSchema#double");
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
