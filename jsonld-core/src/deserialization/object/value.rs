use ld_core::{
    BorrowedRdfTerm,
    CowRdfTerm,
    LinkedData,
    LinkedDataGraph,
    LinkedDataPredicateObjects,
    LinkedDataResource,
    LinkedDataSubject,
    OwnedRdfTerm,
    RdfLiteral,
    ResourceInterpretation,
};
use rdfx::Interpretation;

use crate::{
    Value,
    object::value::Literal,
    rdf::{XSD_BOOLEAN, XSD_CANONICAL_FLOAT, XSD_DOUBLE, XSD_INTEGER, XSD_STRING},
};

/// Builds an owned literal term from a computed lexical value and a datatype
/// IRI. Falls back to `xsd:string` if the IRI is a reserved datatype
/// (`rdf:langString` / `rdf:dirLangString`), which cannot type a plain
/// literal.
fn owned_literal(value: String, ty: iri_rs::Iri<&str>) -> CowRdfTerm<'static> {
    let ty = rdfx::Datatype::new(ty.into()).unwrap_or_else(|_| rdfx::Datatype::xsd_string());
    CowRdfTerm::Owned(OwnedRdfTerm::Literal(RdfLiteral::Any(value, rdfx::LiteralType::Any(ty))))
}

impl<T, I: Interpretation> LinkedDataResource<I> for Value<T>
where
    T: LinkedDataResource<I>,
{
    fn interpretation(&self, interpretation: &mut I) -> ResourceInterpretation<'_, I> {
        let term = match self {
            Self::Json(json) => Some(CowRdfTerm::Owned(OwnedRdfTerm::Literal(RdfLiteral::Json(json.clone())))),
            Self::LangString(lang_string) => {
                let (value, language, _direction) = lang_string.parts();
                match language {
                    // A malformed language tag has no RDF representation, so
                    // the whole literal is dropped (`map` returns `None`)
                    // rather than emitted untagged — same policy as
                    // `RdfQuads`.
                    Some(language) => language.as_well_formed().map(|tag| {
                        CowRdfTerm::Owned(OwnedRdfTerm::Literal(RdfLiteral::Any(
                            value.to_owned(),
                            rdfx::LiteralType::LangString(tag.to_owned()),
                        )))
                    }),
                    None => Some(CowRdfTerm::from_str(value, XSD_STRING)),
                }
            }
            Self::Literal(literal, ty) => {
                // The explicit datatype is only usable when the type resource
                // has a lexical IRI representation.
                let ty_iri = ty.as_ref().and_then(|t| match t.interpretation(interpretation) {
                    ResourceInterpretation::Uninterpreted(Some(CowRdfTerm::Borrowed(BorrowedRdfTerm::Iri(iri)))) => Some(iri),
                    _ => None,
                });

                match literal {
                    Literal::Null => Some(owned_literal("null".to_owned(), ty_iri.unwrap_or(XSD_STRING))),
                    Literal::Boolean(b) => {
                        let value = if *b { "true" } else { "false" };
                        Some(CowRdfTerm::from_str(value, ty_iri.unwrap_or(XSD_BOOLEAN)))
                    }
                    Literal::Number(n) => {
                        let (value, default_ty) = if n.is_i64() && (ty_iri != Some(XSD_DOUBLE)) {
                            (n.to_string(), XSD_INTEGER)
                        } else {
                            (pretty_dtoa::dtoa(n.as_f64_lossy(), XSD_CANONICAL_FLOAT), XSD_DOUBLE)
                        };
                        Some(owned_literal(value, ty_iri.unwrap_or(default_ty)))
                    }
                    Literal::String(s) => Some(CowRdfTerm::from_str(s, ty_iri.unwrap_or(XSD_STRING))),
                }
            }
        };

        ResourceInterpretation::Uninterpreted(term)
    }
}

impl<T, I: Interpretation> LinkedDataSubject<I> for Value<T>
where
    T: LinkedDataResource<I>,
{
    fn visit_subject<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedDataPredicateObjects<I> for Value<T>
where
    T: LinkedDataResource<I>,
{
    fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::PredicateObjectsVisitor<I>,
    {
        visitor.object(self)?;
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedDataGraph<I> for Value<T>
where
    T: LinkedDataResource<I>,
{
    fn visit_graph<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::GraphVisitor<I>,
    {
        visitor.subject(self)?;
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedData<I> for Value<T>
where
    T: LinkedDataResource<I>,
{
    fn visit<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::Visitor<I>,
    {
        visitor.default_graph(self)?;
        visitor.end()
    }
}
