//! This library provides the `test_suite` derive macro
//! that can generate Rust test suites from a JSON-LD document.
use contextual::{DisplayWithContext, WithContext};
use iri_rs::{Iri, IriBuf, IriRefBuf};
use jsonld::{Expand, FsLoader, LoadError};
use proc_macro_error::proc_macro_error;
use proc_macro2::TokenStream;
use quote::quote;
use rdfx::{
    Quad,
    dataset::IndexedBTreeDataset,
    vocabulary::{IriVocabulary, IriVocabularyMut, LiteralIndex},
};
use std::{collections::HashMap, fmt, path::PathBuf};
use syn::{parse::ParseStream, spanned::Spanned};
use tokio::runtime::Builder;

mod vocab;
use vocab::{BlankIdIndex, IndexQuad, IndexTerm, IriIndex, Vocab};
mod ty;
use ty::{Type, UnknownType};

type IndexVocabulary = rdfx::vocabulary::IndexVocabulary;

/// Cache of well-known [`Vocab`] IRIs interned in the working vocabulary.
struct WellKnown {
    rdf_type: IriIndex,
}

impl WellKnown {
    fn new(vocabulary: &mut IndexVocabulary) -> Self {
        Self {
            rdf_type: vocabulary.insert(Iri::<&str>::from(Vocab::Rdf(vocab::Rdf::Type))),
        }
    }
}

struct MountAttribute {
    prefix: IriBuf,
    _comma: syn::token::Comma,
    target: PathBuf,
}

impl syn::parse::Parse for MountAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let prefix: syn::LitStr = input.parse()?;
        let prefix = IriBuf::new(prefix.value()).map_err(|e| input.error(format!("invalid IRI `{}`", e.0)))?;

        let _comma = input.parse()?;

        let target: syn::LitStr = input.parse()?;

        Ok(Self {
            prefix,
            _comma,
            target: target.value().into(),
        })
    }
}

struct IriAttribute {
    iri: IriBuf,
}

impl syn::parse::Parse for IriAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let iri: syn::LitStr = input.parse()?;
        let iri = IriBuf::new(iri.value()).map_err(|e| input.error(format!("invalid IRI `{}`", e.0)))?;

        Ok(Self { iri })
    }
}

struct IriArg {
    iri: IriBuf,
}

impl syn::parse::Parse for IriArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let iri: syn::LitStr = input.parse()?;
        let iri = IriBuf::new(iri.value()).map_err(|e| input.error(format!("invalid IRI `{}`", e.0)))?;

        Ok(Self { iri })
    }
}

struct PrefixBinding {
    prefix: String,
    _eq: syn::token::Eq,
    iri: IriBuf,
}

impl syn::parse::Parse for PrefixBinding {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let prefix: syn::LitStr = input.parse()?;

        let _eq = input.parse()?;

        let iri: syn::LitStr = input.parse()?;
        let iri = IriBuf::new(iri.value()).map_err(|e| input.error(format!("invalid IRI `{}`", e.0)))?;

        Ok(Self {
            prefix: prefix.value(),
            _eq,
            iri,
        })
    }
}

struct IgnoreAttribute {
    iri_ref: IriRefBuf,
    _comma: syn::token::Comma,
    _see: syn::Ident,
    _eq: syn::token::Eq,
    link: String,
}

impl syn::parse::Parse for IgnoreAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let iri_ref: syn::LitStr = input.parse()?;
        let iri_ref = IriRefBuf::new(iri_ref.value()).map_err(|e| input.error(format!("invalid IRI reference `{}`", e.0)))?;

        let _comma = input.parse()?;

        let _see = input.parse()?;

        let _eq = input.parse()?;

        let link: syn::LitStr = input.parse()?;
        let link = link.value();

        Ok(Self {
            iri_ref,
            _comma,
            _see,
            _eq,
            link,
        })
    }
}

struct TestSpec {
    id: syn::Ident,
    prefix: String,
    suite: IriIndex,
    types: HashMap<syn::Ident, ty::Definition>,
    type_map: HashMap<IriIndex, syn::Ident>,
    ignore: HashMap<IriIndex, String>,
}

struct InvalidIri(String);

fn expand_iri(vocabulary: &mut IndexVocabulary, bindings: &mut HashMap<String, IriIndex>, iri: IriBuf) -> Result<IriIndex, InvalidIri> {
    match iri.as_str().split_once(':') {
        Some((prefix, suffix)) => match bindings.get(prefix) {
            Some(prefix) => {
                let mut result = vocabulary.iri(prefix).unwrap().to_string();
                result.push_str(suffix);

                match iri_rs::Iri::parse(result.as_str()) {
                    Ok(iri) => Ok(vocabulary.insert(iri)),
                    Err(_) => Err(InvalidIri(iri.to_string())),
                }
            }
            None => Ok(vocabulary.insert(iri.as_ref())),
        },
        None => Ok(vocabulary.insert(iri.as_ref())),
    }
}

/// Generates one test function per entry of a W3C JSON-LD test manifest.
///
/// The manifest is loaded at compile time, so a manifest the build cannot
/// reach is a compile error rather than a silently empty test run.
#[proc_macro_attribute]
#[proc_macro_error]
pub fn test_suite(args: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::ItemMod);
    let mut vocabulary = IndexVocabulary::new();

    // The manifest loader is synchronous, so a current-thread runtime with no
    // I/O or time driver is all the expansion futures need to make progress.
    let runtime = match Builder::new_current_thread().build() {
        Ok(runtime) => runtime,
        Err(e) => proc_macro_error::abort_call_site!("could not start the async runtime: {}", e),
    };

    match runtime.block_on(derive_test_suite(&mut vocabulary, &mut input, args)) {
        Ok(tokens) => quote! { #input #tokens }.into(),
        Err(e) => {
            proc_macro_error::abort_call_site!("test suite generation failed: {}", (*e).with(&vocabulary))
        }
    }
}

async fn derive_test_suite(vocabulary: &mut IndexVocabulary, input: &mut syn::ItemMod, args: proc_macro::TokenStream) -> Result<TokenStream, Box<Error>> {
    let mut loader = FsLoader::default();
    let spec = parse_input(vocabulary, &mut loader, input, args)?;
    generate_test_suite(vocabulary, loader, spec).await
}

fn parse_input(
    vocabulary: &mut IndexVocabulary,
    loader: &mut FsLoader,
    input: &mut syn::ItemMod,
    args: proc_macro::TokenStream,
) -> Result<TestSpec, Box<Error>> {
    let suite: IriArg = syn::parse(args).map_err(|e| Box::new(e.into()))?;
    let base = suite.iri;
    let suite = vocabulary.insert(base.as_ref());

    let mut bindings: HashMap<String, IriIndex> = HashMap::new();
    let mut ignore: HashMap<IriIndex, String> = HashMap::new();

    let attrs = std::mem::take(&mut input.attrs);
    for attr in attrs {
        if attr.path().is_ident("mount") {
            let mount: MountAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
            let target = if mount.target.is_absolute() {
                mount.target
            } else {
                PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join(mount.target)
            };
            loader.mount(mount.prefix, target)
        } else if attr.path().is_ident("iri_prefix") {
            let attr: PrefixBinding = attr.parse_args().map_err(|e| Box::new(e.into()))?;
            bindings.insert(attr.prefix, vocabulary.insert(attr.iri.as_ref()));
        } else if attr.path().is_ident("ignore_test") {
            let attr: IgnoreAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
            let base_ref = base.as_ref();
            let resolved = attr.iri_ref.resolved(&base_ref).expect("resolved IRI should be valid");
            let resolved_iri: iri_rs::Iri<String> = iri_rs::Iri::try_from(resolved).expect("resolved reference should be an absolute IRI");
            ignore.insert(vocabulary.insert(resolved_iri.as_ref()), attr.link);
        } else {
            input.attrs.push(attr)
        }
    }

    let mut type_map = HashMap::new();
    let mut types = HashMap::new();
    if let Some((_, items)) = input.content.as_mut() {
        for item in items {
            match item {
                syn::Item::Struct(s) => {
                    types.insert(
                        s.ident.clone(),
                        ty::Definition::Struct(parse_struct_type(vocabulary, &mut bindings, &mut type_map, s)?),
                    );
                }
                syn::Item::Enum(e) => {
                    types.insert(
                        e.ident.clone(),
                        ty::Definition::Enum(parse_enum_type(vocabulary, &mut bindings, &mut type_map, e)?),
                    );
                }
                _ => (),
            }
        }
    }

    let prefix = test_prefix(&input.ident.to_string());
    Ok(TestSpec {
        id: input.ident.clone(),
        prefix,
        suite,
        types,
        type_map,
        ignore,
    })
}

fn parse_struct_type(
    vocabulary: &mut IndexVocabulary,
    bindings: &mut HashMap<String, IriIndex>,
    type_map: &mut HashMap<IriIndex, syn::Ident>,
    s: &mut syn::ItemStruct,
) -> Result<ty::Struct, Box<Error>> {
    let mut fields = HashMap::new();

    let attrs = std::mem::take(&mut s.attrs);
    for attr in attrs {
        if attr.path().is_ident("iri") {
            let attr: IriAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
            let iri = expand_iri(vocabulary, bindings, attr.iri).map_err(|e| Box::new(e.into()))?;
            type_map.insert(iri, s.ident.clone());
        } else {
            s.attrs.push(attr)
        }
    }

    for field in &mut s.fields {
        let span = field.span();

        let id = match field.ident.clone() {
            Some(id) => id,
            None => {
                proc_macro_error::abort!(span, "only named fields are supported")
            }
        };

        let mut iri: Option<IriIndex> = None;
        let attrs = std::mem::take(&mut field.attrs);
        for attr in attrs {
            if attr.path().is_ident("iri") {
                let attr: IriAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
                iri = Some(expand_iri(vocabulary, bindings, attr.iri).map_err(|e| Box::new(e.into()))?)
            } else {
                field.attrs.push(attr)
            }
        }

        match iri {
            Some(iri) => {
                let ty_span = field.ty.span();
                match ty::parse(field.ty.clone()) {
                    Ok(ty::Parsed { ty, required, multiple }) => {
                        fields.insert(iri, ty::Field { id, ty, required, multiple });
                    }
                    Err(UnknownType) => {
                        proc_macro_error::abort!(ty_span, "unknown type")
                    }
                }
            }
            None => {
                proc_macro_error::abort!(span, "no IRI specified for field")
            }
        }
    }

    Ok(ty::Struct { fields })
}

fn parse_enum_type(
    vocabulary: &mut IndexVocabulary,
    bindings: &mut HashMap<String, IriIndex>,
    type_map: &mut HashMap<IriIndex, syn::Ident>,
    e: &mut syn::ItemEnum,
) -> Result<ty::Enum, Box<Error>> {
    let mut variants = HashMap::new();

    let attrs = std::mem::take(&mut e.attrs);
    for attr in attrs {
        if attr.path().is_ident("iri") {
            let attr: IriAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
            let iri = expand_iri(vocabulary, bindings, attr.iri).map_err(|e| Box::new(e.into()))?;
            type_map.insert(iri, e.ident.clone());
        } else {
            e.attrs.push(attr)
        }
    }

    for variant in &mut e.variants {
        let span = variant.span();
        let mut iri: Option<IriIndex> = None;
        let attrs = std::mem::take(&mut variant.attrs);
        for attr in attrs {
            if attr.path().is_ident("iri") {
                let attr: IriAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
                iri = Some(expand_iri(vocabulary, bindings, attr.iri).map_err(|e| Box::new(e.into()))?)
            } else {
                variant.attrs.push(attr)
            }
        }

        match iri {
            Some(iri) => {
                let mut fields = HashMap::new();

                for field in &mut variant.fields {
                    let field_span = field.span();
                    let id = match field.ident.clone() {
                        Some(id) => id,
                        None => {
                            proc_macro_error::abort!(field_span, "only named fields are supported")
                        }
                    };

                    let mut field_iri: Option<IriIndex> = None;
                    let attrs = std::mem::take(&mut field.attrs);
                    for attr in attrs {
                        if attr.path().is_ident("iri") {
                            let attr: IriAttribute = attr.parse_args().map_err(|e| Box::new(e.into()))?;
                            field_iri = Some(expand_iri(vocabulary, bindings, attr.iri).map_err(|e| Box::new(e.into()))?)
                        } else {
                            field.attrs.push(attr)
                        }
                    }

                    let field_iri = match field_iri {
                        Some(iri) => iri,
                        None => {
                            proc_macro_error::abort!(field_span, "no IRI specified for field")
                        }
                    };

                    let ty_span = field.ty.span();
                    match ty::parse(field.ty.clone()) {
                        Ok(ty::Parsed { ty, required, multiple }) => {
                            fields.insert(field_iri, ty::Field { id, ty, required, multiple });
                        }
                        Err(UnknownType) => {
                            proc_macro_error::abort!(ty_span, "unknown type")
                        }
                    }
                }

                variants.insert(
                    iri,
                    ty::Variant {
                        id: variant.ident.clone(),
                        data: ty::Struct { fields },
                    },
                );
            }
            None => {
                proc_macro_error::abort!(span, "no IRI specified for variant")
            }
        }
    }

    Ok(ty::Enum { variants })
}

enum Error {
    Parse(syn::Error),
    Load(LoadError<jsonld::loader::fs::Error>),
    Expand(jsonld::expansion::Error<jsonld::loader::fs::Error>),
    InvalidIri(String),
    InvalidValue(Type, IndexTerm),
    InvalidTypeField,
    NoTypeVariants(IndexTerm),
    MultipleTypeVariants(IndexTerm),
}

impl From<syn::Error> for Error {
    fn from(e: syn::Error) -> Self {
        Self::Parse(e)
    }
}

impl From<InvalidIri> for Error {
    fn from(InvalidIri(s): InvalidIri) -> Self {
        Self::InvalidIri(s)
    }
}

impl DisplayWithContext<IndexVocabulary> for Error {
    fn fmt_with(&self, vocabulary: &IndexVocabulary, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Display;
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::Load(e) => e.fmt(f),
            Self::Expand(e) => e.fmt(f),
            Self::InvalidIri(i) => write!(f, "invalid IRI `{i}`"),
            Self::InvalidValue(ty, value) => {
                write!(f, "invalid value {} for type {ty}", value.with(vocabulary))
            }
            Self::InvalidTypeField => write!(f, "invalid type field"),
            Self::NoTypeVariants(r) => {
                write!(f, "no type variants defined for `{}`", r.with(vocabulary))
            }
            Self::MultipleTypeVariants(r) => write!(f, "multiple type variants defined for `{}`", r.with(vocabulary)),
        }
    }
}

async fn generate_test_suite(vocabulary: &mut IndexVocabulary, loader: FsLoader, spec: TestSpec) -> Result<TokenStream, Box<Error>> {
    use jsonld::{Loader, RdfQuads};

    let well_known = WellKnown::new(vocabulary);

    let json_ld = loader.load_with(vocabulary, spec.suite).await.map_err(Error::Load)?;

    let mut expanded_json_ld: jsonld::ExpandedDocument<IriIndex, BlankIdIndex> = json_ld.expand_with(vocabulary, &loader).await.map_err(Error::Expand)?;

    let mut generator = rdfx::generator::Blank::new();
    let _ = expanded_json_ld.identify_all_with(vocabulary, &mut generator);

    let rdf_quads = expanded_json_ld.rdf_quads_with(vocabulary, &mut generator, None);
    let dataset: IndexedBTreeDataset<IndexTerm> = rdf_quads.map(quad_to_owned).collect();

    let mut tests = HashMap::new();

    for Quad(subject, predicate, object, graph) in &dataset {
        if graph.is_none()
            && let IndexTerm::Iri(id) = subject
            && *predicate == IndexTerm::Iri(well_known.rdf_type)
            && let IndexTerm::Iri(ty) = object
            && let Some(type_id) = spec.type_map.get(ty)
        {
            match spec.ignore.get(id) {
                Some(link) => {
                    println!(
                        "    {} test `{}` (see {})",
                        yansi::Paint::yellow("Ignoring").bold(),
                        vocabulary.iri(id).unwrap(),
                        link
                    );
                }
                None => {
                    tests.insert(*id, type_id);
                }
            }
        }
    }

    let id = &spec.id;
    let mut tokens = TokenStream::new();
    for (test, type_id) in tests {
        let ty = spec.types.get(type_id).unwrap();
        let cons = ty.generate(vocabulary, &spec, &dataset, IndexTerm::iri(test), quote! { #id :: #type_id })?;

        let func_name = func_name(&spec.prefix, vocabulary.iri(&test).unwrap().fragment().unwrap());
        let func_id = quote::format_ident!("{}", func_name);

        tokens.extend(quote! {
            #[test]
            fn #func_id() {
                #cons.run()
            }
        })
    }

    Ok(tokens)
}

fn test_prefix(name: &str) -> String {
    let mut segments = Vec::new();
    let mut buffer = String::new();

    for c in name.chars() {
        if c.is_uppercase() && !buffer.is_empty() {
            segments.push(buffer);
            buffer = String::new();
        }

        buffer.push(c.to_lowercase().next().unwrap())
    }

    if !buffer.is_empty() {
        segments.push(buffer)
    }

    if segments.len() > 1 && segments.last().unwrap() == "test" {
        segments.pop();
    }

    let mut result = String::new();

    for segment in segments {
        result.push_str(&segment);
        result.push('_')
    }

    result
}

fn func_name(prefix: &str, id: &str) -> String {
    let mut name = prefix.to_string();
    name.push_str(id);
    name
}

fn quad_to_owned(rdfx::GeneralizedQuad(subject, predicate, object, graph): jsonld::rdf::QuadRef<IriIndex, BlankIdIndex, LiteralIndex>) -> IndexQuad {
    use jsonld::rdf::Value;
    let object_term = match object {
        Value::Id(id) => IndexTerm::from_id(id),
        Value::Literal(l) => IndexTerm::Literal(l),
    };
    Quad(
        IndexTerm::from_id(subject.into_owned()),
        IndexTerm::from_id(predicate.into_owned()),
        object_term,
        graph.cloned().map(IndexTerm::from_id),
    )
}
