use crate::HashSet;
use iri_rs::{Iri, IriBuf, iri};
use mediatype::{
    MediaType,
    MediaTypeBuf,
    names::{APPLICATION, JSON, LD},
};

/// `application/ld+json` media type.
pub const LD_JSON_MEDIA_TYPE: MediaType<'static> = MediaType::from_parts(APPLICATION, LD, Some(JSON), &[]);

/// `application/json` media type.
pub const JSON_MEDIA_TYPE: MediaType<'static> = MediaType::new(APPLICATION, JSON);
use rdfx::vocabulary::{IriVocabulary, IriVocabularyMut};
use std::{borrow::Cow, hash::Hash};

/// Loader combining two loaders, trying each in turn.
pub mod chain;
/// Loader reading documents from the file system.
pub mod fs;
/// Loader serving documents from an in-memory map.
pub mod map;
/// Loader that refuses every request.
pub mod none;

pub use chain::ChainLoader;
pub use fs::FsLoader;
pub use none::NoLoader;

#[cfg(feature = "reqwest")]
pub mod reqwest;

#[cfg(feature = "reqwest")]
pub use self::reqwest::ReqwestLoader;

/// Result of loading a remote document.
pub type LoadingResult<I, E> = Result<RemoteDocument<I>, LoadError<E>>;

/// Reference to a remote context, either inline or by URL.
pub type RemoteContextReference<I = IriBuf> = RemoteDocumentReference<I, jsonld_syntax::Context>;

/// Remote document, loaded or not.
///
/// Either an IRI or the actual document content.
#[derive(Debug, Clone)]
pub enum RemoteDocumentReference<I = IriBuf, T = jstrict::Value> {
    /// IRI to the remote document.
    Iri(I),

    /// Remote document content.
    Loaded(RemoteDocument<I, T>),
}

impl<I, T> RemoteDocumentReference<I, T> {
    /// Creates an IRI to a `jstrict::Value` JSON document.
    ///
    /// This method can replace `RemoteDocumentReference::Iri` to help the type
    /// inference in the case where `T = jstrict::Value`.
    pub fn iri(iri: I) -> Self {
        Self::Iri(iri)
    }
}

impl<I> RemoteDocumentReference<I> {
    /// Loads the remote document with the given `vocabulary` and `loader`.
    ///
    /// If the document is already [`Self::Loaded`], simply returns the inner
    /// [`RemoteDocument`].
    pub async fn load_with<V, L>(self, vocabulary: &mut V, loader: &L) -> LoadingResult<I, L::Error>
    where
        V: IriVocabularyMut<Iri = I>,
        L: Loader,
        I: Clone + Eq + Hash,
    {
        match self {
            Self::Iri(r) => Ok(loader.load_with(vocabulary, r).await?.map(Into::into)),
            Self::Loaded(doc) => Ok(doc),
        }
    }

    /// Loads the remote document with the given `vocabulary` and `loader`.
    ///
    /// For [`Self::Iri`] returns an owned [`RemoteDocument`] with
    /// [`Cow::Owned`].
    /// For [`Self::Loaded`] returns a reference to the inner [`RemoteDocument`]
    /// with [`Cow::Borrowed`].
    pub async fn loaded_with<V, L>(&self, vocabulary: &mut V, loader: &L) -> Result<Cow<'_, RemoteDocument<V::Iri>>, LoadError<L::Error>>
    where
        V: IriVocabularyMut<Iri = I>,
        L: Loader,
        I: Clone + Eq + Hash,
    {
        match self {
            Self::Iri(r) => Ok(Cow::Owned(loader.load_with(vocabulary, r.clone()).await?.map(Into::into))),
            Self::Loaded(doc) => Ok(Cow::Borrowed(doc)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
/// Error raised while loading a remote context.
pub enum ContextLoadError<E> {
    #[error(transparent)]
    /// The document carrying the context could not be loaded.
    LoadingDocumentFailed(#[from] LoadError<E>),

    #[error("context extraction failed")]
    /// Context extraction failed.
    ContextExtractionFailed(#[from] ExtractContextError),
}

impl<I> RemoteContextReference<I> {
    /// Loads the remote context with the given `vocabulary` and `loader`.
    ///
    /// If the context is already [`Self::Loaded`], simply returns the inner
    /// [`RemoteContext`].
    pub async fn load_context_with<V, L: Loader>(self, vocabulary: &mut V, loader: &L) -> Result<RemoteContext<I>, ContextLoadError<L::Error>>
    where
        V: IriVocabularyMut<Iri = I>,
        I: Clone + Eq + Hash,
    {
        match self {
            Self::Iri(r) => Ok(loader.load_with(vocabulary, r).await?.try_map(|d| d.into_ld_context())?),
            Self::Loaded(doc) => Ok(doc),
        }
    }

    /// Loads the remote context with the given `vocabulary` and `loader`.
    ///
    /// For [`Self::Iri`] returns an owned [`RemoteContext`] with
    /// [`Cow::Owned`].
    /// For [`Self::Loaded`] returns a reference to the inner [`RemoteContext`]
    /// with [`Cow::Borrowed`].
    pub async fn loaded_context_with<V, L: Loader>(&self, vocabulary: &mut V, loader: &L) -> Result<Cow<'_, RemoteContext<I>>, ContextLoadError<L::Error>>
    where
        V: IriVocabularyMut<Iri = I>,
        I: Clone + Eq + Hash,
    {
        match self {
            Self::Iri(r) => Ok(Cow::Owned(loader.load_with(vocabulary, r.clone()).await?.try_map(|d| d.into_ld_context())?)),
            Self::Loaded(doc) => Ok(Cow::Borrowed(doc)),
        }
    }
}

/// Remote document.
///
/// Stores the content of a loaded remote document along with its original URL.
#[derive(Debug, Clone)]
pub struct RemoteDocument<I = IriBuf, T = jstrict::Value> {
    /// The final URL of the loaded document, after eventual redirection.
    pub url: Option<I>,

    /// The HTTP `Content-Type` header value of the loaded document, exclusive
    /// of any optional parameters.
    pub content_type: Option<MediaTypeBuf>,

    /// If available, the value of the HTTP `Link Header` [RFC 8288] using the
    /// `http://www.w3.org/ns/json-ld#context` link relation in the response.
    ///
    /// If the response's `Content-Type` is `application/ld+json`, the HTTP
    /// `Link Header` is ignored. If multiple HTTP `Link Headers` using the
    /// `http://www.w3.org/ns/json-ld#context` link relation are found, the
    /// loader fails with a `multiple context link headers` error.
    ///
    /// [RFC 8288]: https://www.rfc-editor.org/rfc/rfc8288
    pub context_url: Option<I>,

    /// Profiles advertised by the document's media type.
    pub profile: HashSet<Profile<I>>,

    /// The retrieved document.
    pub document: T,
}

/// Remote document holding a context.
pub type RemoteContext<I = IriBuf> = RemoteDocument<I, jsonld_syntax::context::Context>;

impl<I, T> RemoteDocument<I, T> {
    /// Creates a new remote document.
    ///
    /// `url` is the final URL of the loaded document, after eventual
    /// redirection.
    /// `content_type` is the HTTP `Content-Type` header value of the loaded
    /// document, exclusive of any optional parameters.
    pub fn new(url: Option<I>, content_type: Option<MediaTypeBuf>, document: T) -> Self {
        Self::new_full(url, content_type, None, HashSet::default(), document)
    }

    /// Creates a new remote document.
    ///
    /// `url` is the final URL of the loaded document, after eventual
    /// redirection.
    /// `content_type` is the HTTP `Content-Type` header value of the loaded
    /// document, exclusive of any optional parameters.
    /// `context_url` is the value of the HTTP `Link Header` [RFC 8288] using the
    /// `http://www.w3.org/ns/json-ld#context` link relation in the response,
    /// if any.
    /// `profile` is the value of any profile parameter retrieved as part of the
    /// original contentType.
    ///
    /// [RFC 8288]: https://www.rfc-editor.org/rfc/rfc8288
    pub fn new_full(url: Option<I>, content_type: Option<MediaTypeBuf>, context_url: Option<I>, profile: HashSet<Profile<I>>, document: T) -> Self {
        Self {
            url,
            content_type,
            context_url,
            profile,
            document,
        }
    }

    /// Maps the content of the remote document.
    pub fn map<U>(self, f: impl Fn(T) -> U) -> RemoteDocument<I, U> {
        RemoteDocument {
            url: self.url,
            content_type: self.content_type,
            context_url: self.context_url,
            profile: self.profile,
            document: f(self.document),
        }
    }

    /// Tries to map the content of the remote document.
    pub fn try_map<U, E>(self, f: impl Fn(T) -> Result<U, E>) -> Result<RemoteDocument<I, U>, E> {
        Ok(RemoteDocument {
            url: self.url,
            content_type: self.content_type,
            context_url: self.context_url,
            profile: self.profile,
            document: f(self.document)?,
        })
    }

    /// Maps all the IRIs.
    pub fn map_iris<J>(self, mut f: impl FnMut(I) -> J) -> RemoteDocument<J, T>
    where
        J: Eq + Hash,
    {
        RemoteDocument {
            url: self.url.map(&mut f),
            content_type: self.content_type,
            context_url: self.context_url.map(&mut f),
            profile: self.profile.into_iter().map(|p| p.map_iri(&mut f)).collect(),
            document: self.document,
        }
    }

    /// Returns a reference to the final URL of the loaded document, after eventual redirection.
    pub fn url(&self) -> Option<&I> {
        self.url.as_ref()
    }

    /// Returns the HTTP `Content-Type` header value of the loaded document,
    /// exclusive of any optional parameters.
    pub fn content_type(&self) -> Option<&MediaTypeBuf> {
        self.content_type.as_ref()
    }

    /// Returns the value of the HTTP `Link Header` [RFC 8288] using the
    /// `http://www.w3.org/ns/json-ld#context` link relation in the response,
    /// if any.
    ///
    /// If the response's `Content-Type` is `application/ld+json`, the HTTP
    /// `Link Header` is ignored. If multiple HTTP `Link Headers` using the
    /// `http://www.w3.org/ns/json-ld#context` link relation are found, the
    /// loader fails with a `multiple context link headers` error.
    ///
    /// [RFC 8288]: https://www.rfc-editor.org/rfc/rfc8288
    pub fn context_url(&self) -> Option<&I> {
        self.context_url.as_ref()
    }

    /// Returns a reference to the content of the document.
    pub fn document(&self) -> &T {
        &self.document
    }

    /// Returns a mutable reference to the content of the document.
    pub fn document_mut(&mut self) -> &mut T {
        &mut self.document
    }

    /// Drops the original URL and returns the content of the document.
    pub fn into_document(self) -> T {
        self.document
    }

    /// Drops the content and returns the original URL of the document.
    pub fn into_url(self) -> Option<I> {
        self.url
    }

    /// Sets the URL of the document.
    pub fn set_url(&mut self, url: Option<I>) {
        self.url = url
    }
}

impl<I> RemoteDocument<I, jstrict::Value> {
    /// Creates a remote document from any value convertible into a
    /// [`jstrict::Value`].
    ///
    /// With the `serde_json` feature enabled, this also accepts
    /// `serde_json::Value` thanks to the `From<serde_json::Value>`
    /// implementation provided by `jstrict`.
    pub fn from_value(url: Option<I>, content_type: Option<MediaTypeBuf>, document: impl Into<jstrict::Value>) -> Self {
        Self::new(url, content_type, document.into())
    }
}

#[cfg(feature = "serde-json")]
impl<I> RemoteDocument<I, jstrict::Value> {
    /// Creates a remote document from a [`serde_json::Value`].
    pub fn from_serde_json(url: Option<I>, content_type: Option<MediaTypeBuf>, document: serde_json::Value) -> Self {
        Self::new(url, content_type, jstrict::Value::from_serde_json(document))
    }

    /// Consumes the document, returning a `RemoteDocument<I, serde_json::Value>`.
    pub fn into_serde_json(self) -> RemoteDocument<I, serde_json::Value> {
        self.map(jstrict::Value::into_serde_json)
    }
}

/// Standard `profile` parameter values defined for the `application/ld+json`.
///
/// See: <https://www.w3.org/TR/json-ld11/#iana-considerations>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StandardProfile {
    /// To request or specify expanded JSON-LD document form.
    Expanded,

    /// To request or specify compacted JSON-LD document form.
    Compacted,

    /// To request or specify a JSON-LD context document.
    Context,

    /// To request or specify flattened JSON-LD document form.
    Flattened,

    /// To request or specify a JSON-LD framed document.
    Framed,
}

impl StandardProfile {
    /// Returns the standard profile denoted by `iri`, if any.
    pub fn from_iri(iri: Iri<&str>) -> Option<Self> {
        if iri == iri!("http://www.w3.org/ns/json-ld#expanded") {
            Some(Self::Expanded)
        } else if iri == iri!("http://www.w3.org/ns/json-ld#compacted") {
            Some(Self::Compacted)
        } else if iri == iri!("http://www.w3.org/ns/json-ld#context") {
            Some(Self::Context)
        } else if iri == iri!("http://www.w3.org/ns/json-ld#flattened") {
            Some(Self::Flattened)
        } else if iri == iri!("http://www.w3.org/ns/json-ld#framed") {
            Some(Self::Framed)
        } else {
            None
        }
    }

    /// Returns the IRI that identifies this profile.
    pub fn iri(&self) -> Iri<&'static str> {
        match self {
            Self::Expanded => iri!("http://www.w3.org/ns/json-ld#expanded"),
            Self::Compacted => iri!("http://www.w3.org/ns/json-ld#compacted"),
            Self::Context => iri!("http://www.w3.org/ns/json-ld#context"),
            Self::Flattened => iri!("http://www.w3.org/ns/json-ld#flattened"),
            Self::Framed => iri!("http://www.w3.org/ns/json-ld#framed"),
        }
    }
}

/// Value of the `profile` parameter of the `application/ld+json` media type.
///
/// The values the JSON-LD specification itself registers are enumerated by
/// [`StandardProfile`]; any other IRI is carried as [`Profile::Custom`].
///
/// See: <https://www.w3.org/TR/json-ld11/#iana-considerations>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Profile<I = IriBuf> {
    /// A profile registered by the JSON-LD specification.
    Standard(StandardProfile),
    /// A profile outside the ones the specification registers.
    Custom(I),
}

impl Profile {
    /// Builds a profile from an IRI, recognising the ones the specification
    /// registers.
    pub fn new(iri: Iri<&str>) -> Self {
        match StandardProfile::from_iri(iri) {
            Some(p) => Self::Standard(p),
            None => Self::Custom(iri.into()),
        }
    }

    /// Returns the IRI that identifies this profile.
    pub fn iri(&self) -> Iri<&str> {
        match self {
            Self::Standard(s) => s.iri(),
            Self::Custom(c) => c.as_ref(),
        }
    }
}

impl<I> Profile<I> {
    /// Builds a profile from an IRI, interning it in the given vocabulary.
    pub fn new_with(iri: Iri<&str>, vocabulary: &mut impl IriVocabularyMut<Iri = I>) -> Self {
        match StandardProfile::from_iri(iri) {
            Some(p) => Self::Standard(p),
            None => Self::Custom(vocabulary.insert(iri)),
        }
    }

    /// Returns the [`Iri`] of this profile, if it can be resolved by the given
    /// `vocabulary`.
    pub fn iri_with<'a>(&'a self, vocabulary: &'a impl IriVocabulary<Iri = I>) -> Option<Iri<&'a str>> {
        match self {
            Self::Standard(s) => Some(s.iri()),
            Self::Custom(c) => vocabulary.iri(c),
        }
    }

    /// Rewrites the IRI of a custom profile.
    pub fn map_iri<J>(self, f: impl FnOnce(I) -> J) -> Profile<J> {
        match self {
            Self::Standard(p) => Profile::Standard(p),
            Self::Custom(i) => Profile::Custom(f(i)),
        }
    }
}

/// Loading error: wraps the loader-specific error with the IRI we tried to
/// load.
#[derive(Debug, thiserror::Error)]
#[error("loading document `{target}` failed: {source}")]
/// Error raised while loading a remote document, with the URL that failed.
pub struct LoadError<E> {
    /// URL whose loading failed.
    pub target: IriBuf,
    #[source]
    /// Underlying loader error.
    pub source: E,
}

impl<E> LoadError<E> {
    /// Creates a new `LoadError`.
    pub fn new(target: IriBuf, source: E) -> Self {
        Self { target, source }
    }

    /// Rewrites the underlying error, keeping the URL that failed.
    pub fn map_source<F, U>(self, f: F) -> LoadError<U>
    where
        F: FnOnce(E) -> U,
    {
        LoadError {
            target: self.target,
            source: f(self.source),
        }
    }
}

/// Document loader.
///
/// A document loader is required by most processing functions to fetch remote
/// documents identified by an IRI. In particular, the loader is in charge of
/// fetching all the remote contexts imported in a `@context` entry.
///
/// This library provides a few default loader implementations:
///   - [`NoLoader`] dummy loader that always fail. Perfect if you are certain
///     that the processing will not require any loading.
///   - Standard [`HashMap`](std::collections::HashMap) and
///     [`BTreeMap`](std::collections::BTreeMap) mapping IRIs to pre-loaded
///     documents. This way no network calls are performed and the loaded
///     content can be trusted.
///   - [`FsLoader`] that redirecting registered IRI prefixes to a local
///     directory on the file system. This also avoids network calls. The loaded
///     content can be trusted as long as the file system is trusted.
///   - `ReqwestLoader` actually downloading the remote documents using the
///     [`reqwest`](https://crates.io/crates/reqwest) library.
///     This requires the `reqwest` feature to be enabled.
pub trait Loader {
    /// Loader-specific error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Loads the document behind the given IRI, using the given vocabulary.
    ///
    /// # Panics
    ///
    /// Panics if `url` does not resolve in `vocabulary`, i.e. if it was
    /// obtained from a different vocabulary.
    async fn load_with<V>(&self, vocabulary: &mut V, url: V::Iri) -> LoadingResult<V::Iri, Self::Error>
    where
        V: IriVocabularyMut,
        V::Iri: Clone + Eq + Hash,
    {
        #[allow(clippy::expect_used)]
        let lexical_url = vocabulary.iri(&url).expect("`url` does not resolve in the given vocabulary");
        let document = self.load(lexical_url).await?;
        Ok(document.map_iris(|i| vocabulary.insert_owned(i)))
    }

    /// Loads the document behind the given IRI.
    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>>;
}

impl<L: Loader> Loader for &L {
    type Error = L::Error;

    async fn load_with<V>(&self, vocabulary: &mut V, url: V::Iri) -> LoadingResult<V::Iri, Self::Error>
    where
        V: IriVocabularyMut,
        V::Iri: Clone + Eq + Hash,
    {
        L::load_with(self, vocabulary, url).await
    }

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        L::load(self, url).await
    }
}

impl<L: Loader> Loader for &mut L {
    type Error = L::Error;

    async fn load_with<V>(&self, vocabulary: &mut V, url: V::Iri) -> LoadingResult<V::Iri, Self::Error>
    where
        V: IriVocabularyMut,
        V::Iri: Clone + Eq + Hash,
    {
        L::load_with(self, vocabulary, url).await
    }

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        L::load(self, url).await
    }
}

/// Context extraction error.
#[derive(Debug, thiserror::Error)]
pub enum ExtractContextError {
    /// Unexpected JSON value.
    #[error("unexpected {0}")]
    Unexpected(jstrict::Kind),

    /// No context definition found.
    #[error("missing `@context` entry")]
    NoContext,

    /// Multiple context definitions found.
    #[error("duplicate `@context` entry")]
    DuplicateContext,

    /// JSON syntax error.
    #[error("JSON-LD context syntax error: {0}")]
    Syntax(jsonld_syntax::context::InvalidContext),
}

impl ExtractContextError {
    fn duplicate_context(jstrict::object::Duplicate(_, _): jstrict::object::Duplicate<jstrict::object::Entry>) -> Self {
        Self::DuplicateContext
    }
}

/// Documents from which a JSON-LD context can be extracted.
pub trait ExtractContext {
    /// Consumes this `ExtractContext`, returning its LD context.
    fn into_ld_context(self) -> Result<jsonld_syntax::context::Context, ExtractContextError>;
}

impl ExtractContext for jstrict::Value {
    fn into_ld_context(self) -> Result<jsonld_syntax::context::Context, ExtractContextError> {
        match self {
            Self::Object(mut o) => match o.remove_unique("@context").map_err(ExtractContextError::duplicate_context)? {
                Some(context) => {
                    use jsonld_syntax::TryFromJson;
                    jsonld_syntax::context::Context::try_from_json(&context.value).map_err(ExtractContextError::Syntax)
                }
                None => Err(ExtractContextError::NoContext),
            },
            other => Err(ExtractContextError::Unexpected(other.kind())),
        }
    }
}

#[cfg(all(test, feature = "serde-json"))]
mod serde_json_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use iri_rs::iri;

    #[test]
    fn from_serde_json_preserves_url_and_content_type() {
        let url = IriBuf::from(iri!("https://example.com/sample.jsonld"));
        let mime: MediaTypeBuf = LD_JSON_MEDIA_TYPE.into();
        let value = serde_json::json!({"foo": "bar"});

        let doc = RemoteDocument::from_serde_json(Some(url.clone()), Some(mime.clone()), value);

        assert_eq!(doc.url(), Some(&url));
        assert_eq!(doc.content_type(), Some(&mime));
        assert!(doc.context_url().is_none());
        assert!(doc.profile.is_empty());
        match doc.document() {
            jstrict::Value::Object(o) => {
                assert_eq!(o.get("foo").next().unwrap().as_str(), Some("bar"));
            }
            other => panic!("expected object, got {:?}", other.kind()),
        }
    }

    #[test]
    fn from_serde_json_handles_all_value_kinds() {
        let value = serde_json::json!({
            "null": null,
            "bool": true,
            "num": 1.5,
            "str": "x",
            "arr": [1, 2],
            "obj": {"k": "v"}
        });
        let doc = RemoteDocument::<IriBuf, _>::from_serde_json(None, None, value);
        let obj = match doc.document() {
            jstrict::Value::Object(o) => o,
            _ => panic!("expected object"),
        };
        assert!(matches!(obj.get("null").next().unwrap(), jstrict::Value::Null));
        assert!(matches!(obj.get("bool").next().unwrap(), jstrict::Value::Boolean(true)));
        assert!(matches!(obj.get("num").next().unwrap(), jstrict::Value::Number(_)));
        assert!(matches!(obj.get("str").next().unwrap(), jstrict::Value::String(_)));
        assert!(matches!(obj.get("arr").next().unwrap(), jstrict::Value::Array(_)));
        assert!(matches!(obj.get("obj").next().unwrap(), jstrict::Value::Object(_)));
    }

    #[test]
    fn into_serde_json_round_trip() {
        use jstrict::Parse;
        let original = jstrict::Value::parse_str(r#"{"a": [1, 2, 3], "b": "x"}"#).unwrap().0;
        let doc: RemoteDocument<IriBuf, _> = RemoteDocument::new(None, None, original.clone());
        let serde_doc = doc.into_serde_json();
        let round_tripped = jstrict::Value::from_serde_json(serde_doc.document.clone());
        assert_eq!(round_tripped, original);
    }

    #[test]
    fn from_value_accepts_jstrict_value() {
        use jstrict::Parse;
        let value = jstrict::Value::parse_str(r#"{"k": 1}"#).unwrap().0;
        let doc: RemoteDocument<IriBuf, _> = RemoteDocument::from_value(None, None, value);
        match doc.document() {
            jstrict::Value::Object(_) => {}
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn from_value_accepts_serde_json_value() {
        let v = serde_json::json!({"k": 1});
        let via_value: RemoteDocument<IriBuf, _> = RemoteDocument::from_value(None, None, v.clone());
        let via_serde: RemoteDocument<IriBuf, _> = RemoteDocument::from_serde_json(None, None, v);
        assert!(jsonld_syntax::Compare::compare(via_value.document(), via_serde.document()));
    }
}
