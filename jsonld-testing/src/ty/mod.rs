use crate::{Error, IndexVocabulary, IriIndex, TestSpec, Vocab, vocab, vocab::IndexTerm};
use core::fmt;
use iri_rs::Iri;
use proc_macro2::TokenStream;
use quote::quote;
use rdf_rs::{
    dataset::{IndexedBTreeDataset, PatternMatchingDataset},
    vocabulary::{BlankIdVocabulary, IriVocabulary, LiteralVocabulary},
};
use std::collections::HashMap;

mod parse;

pub use parse::{Parsed, UnknownType, parse};

pub enum Definition {
    Struct(Struct),
    Enum(Enum),
}

pub struct Struct {
    pub fields: HashMap<IriIndex, Field>,
}

pub struct Field {
    pub id: syn::Ident,
    pub ty: Type,
    pub required: bool,
    pub multiple: bool,
}

pub struct Enum {
    pub variants: HashMap<IriIndex, Variant>,
}

pub struct Variant {
    pub id: syn::Ident,
    pub data: Struct,
}

#[derive(Clone)]
pub enum Type {
    Bool,
    String,
    Iri,
    ProcessingMode,
    RdfDirection,
    Ref(syn::Ident),
}

impl Type {
    fn is_id(&self) -> bool {
        matches!(self, Self::Ref(_))
    }

    fn as_id(&self) -> Option<&syn::Ident> {
        match self {
            Self::Ref(r) => Some(r),
            _ => None,
        }
    }

    pub(crate) fn generate(
        &self,
        vocabulary: &IndexVocabulary,
        spec: &TestSpec,
        dataset: &IndexedBTreeDataset<IndexTerm>,
        value: &IndexTerm,
    ) -> Result<TokenStream, Box<Error>> {
        match self {
            Self::Bool => {
                let b = match value {
                    IndexTerm::Literal(l) => {
                        let literal = vocabulary.literal(l).unwrap();
                        let xsd_boolean: Iri<&str> = Iri::<&str>::from(Vocab::Xsd(vocab::Xsd::Boolean));
                        if literal.type_.is_iri(&xsd_boolean) {
                            match literal.value {
                                "true" => true,
                                "false" => false,
                                _ => return Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                            }
                        } else {
                            return Err(Box::new(Error::InvalidValue(self.clone(), value.clone())));
                        }
                    }
                    _ => return Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                };

                Ok(quote! { #b })
            }
            Self::String => {
                let s: String = match value {
                    IndexTerm::Literal(lit) => vocabulary.literal(lit).unwrap().value.to_owned(),
                    IndexTerm::Iri(i) => vocabulary.iri(i).unwrap().as_str().to_owned(),
                    IndexTerm::Blank(b) => vocabulary.blank_id(b).unwrap().as_str().to_owned(),
                };

                Ok(quote! { #s })
            }
            Self::Iri => match value {
                IndexTerm::Iri(i) => {
                    let iri = vocabulary.iri(i).unwrap();
                    let s = iri.as_str();
                    Ok(quote! { ::iri_rs::iri!(#s) })
                }
                _ => Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
            },
            Self::ProcessingMode => {
                let s = match value {
                    IndexTerm::Literal(l) => {
                        let literal = vocabulary.literal(l).unwrap();
                        let xsd_string: Iri<&str> = Iri::<&str>::from(Vocab::Xsd(vocab::Xsd::String));
                        if literal.type_.is_iri(&xsd_string) {
                            literal.value.to_owned()
                        } else {
                            return Err(Box::new(Error::InvalidValue(self.clone(), value.clone())));
                        }
                    }
                    _ => return Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                };

                match jsonld::ProcessingMode::try_from(s.as_str()) {
                    Ok(p) => match p {
                        jsonld::ProcessingMode::JsonLd1_0 => Ok(quote! { ::jsonld::ProcessingMode::JsonLd1_0 }),
                        jsonld::ProcessingMode::JsonLd1_1 => Ok(quote! { ::jsonld::ProcessingMode::JsonLd1_1 }),
                    },
                    Err(_) => Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                }
            }
            Self::RdfDirection => {
                let s = match value {
                    IndexTerm::Literal(l) => {
                        let literal = vocabulary.literal(l).unwrap();
                        let xsd_string: Iri<&str> = Iri::<&str>::from(Vocab::Xsd(vocab::Xsd::String));
                        if literal.type_.is_iri(&xsd_string) {
                            literal.value.to_owned()
                        } else {
                            return Err(Box::new(Error::InvalidValue(self.clone(), value.clone())));
                        }
                    }
                    _ => return Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                };

                match jsonld::rdf::RdfDirection::try_from(s.as_str()) {
                    Ok(p) => match p {
                        jsonld::rdf::RdfDirection::CompoundLiteral => Ok(quote! { ::jsonld::rdf::RdfDirection::CompoundLiteral }),
                        jsonld::rdf::RdfDirection::I18nDatatype => Ok(quote! { ::jsonld::rdf::RdfDirection::I18nDatatype }),
                    },
                    Err(_) => Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
                }
            }
            Self::Ref(r) => match value {
                IndexTerm::Iri(_) | IndexTerm::Blank(_) => {
                    let d = spec.types.get(r).unwrap();
                    let mod_id = &spec.id;
                    d.generate(vocabulary, spec, dataset, value.clone(), quote! { #mod_id :: #r })
                }
                _ => Err(Box::new(Error::InvalidValue(self.clone(), value.clone()))),
            },
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => write!(f, "bool"),
            Self::String => write!(f, "str"),
            Self::Iri => write!(f, "Iri"),
            Self::ProcessingMode => write!(f, "ProcessingMode"),
            Self::RdfDirection => write!(f, "RdfDirection"),
            Self::Ref(id) => id.fmt(f),
        }
    }
}

impl Struct {
    pub(crate) fn generate(
        &self,
        vocabulary: &IndexVocabulary,
        spec: &TestSpec,
        dataset: &IndexedBTreeDataset<IndexTerm>,
        id: IndexTerm,
        path: TokenStream,
    ) -> Result<TokenStream, Box<Error>> {
        let mut fields = Vec::new();

        let rdf_type_iri = Iri::<&str>::from(Vocab::Rdf(vocab::Rdf::Type));
        let rdf_type_index = vocabulary.get(rdf_type_iri);

        for (field_iri, field) in &self.fields {
            let ident = &field.id;
            let value = if Some(*field_iri) == rdf_type_index && field.ty.is_id() {
                if field.multiple || !field.required {
                    return Err(Box::new(Error::InvalidTypeField));
                }

                let ty_id = field.ty.as_id().expect("not a reference");
                let ty = spec.types.get(ty_id).expect("undefined type");
                let mod_id = &spec.id;
                ty.generate(vocabulary, spec, dataset, id, quote! { #mod_id :: #ty_id })?
            } else {
                let field_predicate = IndexTerm::iri(*field_iri);
                let mut objects = dataset.quad_objects(None, &id, &field_predicate);

                if field.multiple {
                    let mut items = Vec::new();

                    for object in objects {
                        items.push(field.ty.generate(vocabulary, spec, dataset, object)?)
                    }

                    quote! {
                        &[ #(#items),* ]
                    }
                } else if field.required {
                    match objects.next() {
                        Some(object) => field.ty.generate(vocabulary, spec, dataset, object)?,
                        // None => return Err(Error::MissingRequiredValue(id, *field_iri))
                        None => {
                            quote! { ::core::default::Default::default() }
                        }
                    }
                } else {
                    match objects.next() {
                        Some(object) => {
                            let value = field.ty.generate(vocabulary, spec, dataset, object)?;
                            quote! { Some(#value) }
                        }
                        None => quote! { None },
                    }
                }
            };

            fields.push(quote! { #ident: #value })
        }

        Ok(quote! { #path { #(#fields),* } })
    }
}

impl Definition {
    pub(crate) fn generate(
        &self,
        vocabulary: &IndexVocabulary,
        spec: &TestSpec,
        dataset: &IndexedBTreeDataset<IndexTerm>,
        id: IndexTerm,
        path: TokenStream,
    ) -> Result<TokenStream, Box<Error>> {
        match self {
            Self::Struct(s) => s.generate(vocabulary, spec, dataset, id, path),
            Self::Enum(e) => {
                let mut variant = None;
                let rdf_type_iri = Iri::<&str>::from(Vocab::Rdf(vocab::Rdf::Type));
                let rdf_type_index = vocabulary.get(rdf_type_iri);

                if let Some(rdf_type_index) = rdf_type_index {
                    let predicate = IndexTerm::iri(rdf_type_index);
                    let node_types = dataset.quad_objects(None, &id, &predicate);

                    for ty_iri in node_types {
                        match ty_iri {
                            IndexTerm::Iri(ty_iri) => {
                                if let Some(v) = e.variants.get(ty_iri) {
                                    if variant.replace(v).is_some() {
                                        return Err(Box::new(Error::MultipleTypeVariants(id)));
                                    }
                                }
                            }
                            _ => panic!("invalid type"),
                        }
                    }
                }

                match variant {
                    Some(variant) => {
                        let variant_id = &variant.id;
                        variant.data.generate(vocabulary, spec, dataset, id, quote! { #path :: #variant_id })
                    }
                    None => Err(Box::new(Error::NoTypeVariants(id))),
                }
            }
        }
    }
}
