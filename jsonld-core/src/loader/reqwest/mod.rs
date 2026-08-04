//! Simple document and context loader based on [`reqwest`](https://crates.io/crates/reqwest)
use crate::{LoadError, Profile};

use super::{LD_JSON_MEDIA_TYPE, Loader, RemoteDocument};
use crate::HashSet;
use iri_rs::{Iri, IriBuf};
use jstrict::Parse;
use reqwest::{
    StatusCode,
    header::{ACCEPT, CONTENT_TYPE, LINK},
};
use reqwest_middleware::ClientWithMiddleware;

mod content_type;
mod link;

use content_type::ContentType;
use link::Link;

/// Loader options.
pub struct Options {
    /// One or more IRIs to use in the request as a profile parameter.
    ///
    /// (See [IANA Considerations](https://www.w3.org/TR/json-ld11/#iana-considerations)).
    pub request_profile: Vec<Profile>,

    /// Maximum number of allowed `Link` header redirections before the loader
    /// fails.
    ///
    /// Defaults to 8.
    ///
    /// Note: this only controls how many times the loader will use a `Link`
    /// HTTP header to find the target JSON-LD document. The number of allowed
    /// regular HTTP redirections is controlled by the HTTP
    /// [`client`](Self::client).
    pub max_redirections: usize,

    /// Maximum size of a loaded document, in bytes.
    ///
    /// Responses larger than this fail with [`Error::TooLarge`] instead of
    /// buffering unbounded amounts of memory. Defaults to 32 MiB. Set to
    /// `None` to disable the limit.
    pub max_document_size: Option<usize>,

    /// Timeout applied to each HTTP request, from connection to the end of
    /// the response body.
    ///
    /// Defaults to 60 seconds, so a hung server cannot stall loading
    /// indefinitely. Set to `None` to defer to the [`client`](Self::client)
    /// configuration.
    pub timeout: Option<std::time::Duration>,

    /// HTTP client.
    pub client: ClientWithMiddleware,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            request_profile: Vec::new(),
            max_redirections: 8,
            max_document_size: Some(32 * 1024 * 1024),
            timeout: Some(std::time::Duration::from_mins(1)),
            client: reqwest_middleware::ClientBuilder::new(reqwest::Client::default()).build(),
        }
    }
}

/// Loading error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    /// The HTTP request could not be completed — a connection, TLS, timeout
    /// or middleware failure.
    Reqwest(reqwest_middleware::Error),

    #[error("query failed: status code {0}")]
    /// The server answered with a non-success status code.
    QueryFailed(StatusCode),

    #[error("invalid content type")]
    /// The response `Content-Type` is neither `application/ld+json` nor any
    /// other JSON media type the loader accepts.
    InvalidContentType,

    #[error("multiple context link headers")]
    /// The response carried more than one `Link` header pointing at a
    /// JSON-LD context, leaving the target document ambiguous.
    MultipleContextLinkHeaders,

    #[error("too many redirections")]
    /// The loader followed as many `Link` headers as `Options::max_redirections`
    /// allows without reaching a JSON-LD document.
    TooManyRedirections,

    #[error("document exceeds the size limit ({0} bytes)")]
    /// The response body exceeded the `Options::max_document_size` limit.
    /// Carries that limit, in bytes.
    TooLarge(usize),

    #[error("JSON parse error: {0}")]
    /// The response body is not well-formed JSON.
    Parse(jstrict::parse::Error<utf8_decode::Utf8Error>),
}

/// `reqwest`-based loader.
///
/// Only works with the [`tokio`](https://tokio.rs/) runtime.
///
/// Follows HTTP redirections and JSON-LD `Link` headers, up to
/// [`Options::max_redirections`].
///
/// Loaded documents are not cached: a new network request is made each time a
/// URL is loaded, even if it has been fetched before.
pub struct ReqwestLoader {
    options: Options,
    accept_header: String,
}

impl Default for ReqwestLoader {
    fn default() -> Self {
        Self::new_using(Options::default())
    }
}

impl ReqwestLoader {
    /// Creates a new loader with the default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new loader with the given options.
    #[must_use]
    pub fn new_using(options: Options) -> Self {
        let mut json_ld_params = String::new();

        if !options.request_profile.is_empty() {
            json_ld_params.push_str("; profile=");

            if options.request_profile.len() > 1 {
                json_ld_params.push('"');
            }

            for (i, p) in options.request_profile.iter().enumerate() {
                if i > 0 {
                    json_ld_params.push(' ');
                }

                json_ld_params.push_str(p.iri().as_str());
            }

            if options.request_profile.len() > 1 {
                json_ld_params.push('"');
            }
        }

        Self {
            options,
            accept_header: format!("application/ld+json{json_ld_params}, application/json"),
        }
    }
}

impl Loader for ReqwestLoader {
    type Error = Error;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        let mut redirection_number = 0;
        let mut url: IriBuf = url.into();
        'next_url: loop {
            if redirection_number > self.options.max_redirections {
                return Err(LoadError::new(url.clone(), Error::TooManyRedirections));
            }

            log::debug!("downloading: {url}");
            let mut request = self.options.client.get(url.as_str()).header(ACCEPT, &self.accept_header);

            if let Some(timeout) = self.options.timeout {
                request = request.timeout(timeout);
            }

            let mut response = request.send().await.map_err(|e| LoadError::new(url.clone(), Error::Reqwest(e)))?;

            match response.status() {
                StatusCode::OK => {
                    let mut content_types = response.headers().get_all(CONTENT_TYPE).into_iter().filter_map(ContentType::new);

                    if let Some(content_type) = content_types.find(ContentType::is_json_ld) {
                        let mut context_url = None;
                        if *content_type.media_type() != LD_JSON_MEDIA_TYPE {
                            for link in response.headers().get_all(LINK).into_iter().flat_map(Link::parse_header) {
                                if link.rel() == Some(b"http://www.w3.org/ns/json-ld#context") {
                                    if context_url.is_some() {
                                        return Err(LoadError::new(url, Error::MultipleContextLinkHeaders));
                                    }

                                    if let Ok(resolved) = link.href().resolved(&url)
                                        && let Ok(iri) = IriBuf::try_from(resolved)
                                    {
                                        context_url = Some(iri);
                                    }
                                }
                            }
                        }

                        let mut profile = HashSet::default();
                        for p in content_type.profile().into_iter().flat_map(|p| p.split(|b| *b == b' ')) {
                            if let Ok(p) = std::str::from_utf8(p)
                                && let Ok(iri) = Iri::parse(p)
                            {
                                profile.insert(Profile::new(iri));
                            }
                        }

                        if let (Some(limit), Some(len)) = (self.options.max_document_size, response.content_length())
                            && len > limit as u64
                        {
                            return Err(LoadError::new(url, Error::TooLarge(limit)));
                        }

                        let mut bytes = Vec::new();
                        while let Some(chunk) = response.chunk().await.map_err(|e| LoadError::new(url.clone(), Error::Reqwest(e.into())))? {
                            if let Some(limit) = self.options.max_document_size
                                && bytes.len() + chunk.len() > limit
                            {
                                return Err(LoadError::new(url, Error::TooLarge(limit)));
                            }

                            bytes.extend_from_slice(&chunk);
                        }

                        let decoder = utf8_decode::Decoder::new(bytes.iter().copied());
                        let (document, _) = jstrict::Value::parse_utf8(decoder).map_err(|e| LoadError::new(url.clone(), Error::Parse(e)))?;

                        break Ok(RemoteDocument::new_full(
                            Some(url),
                            Some(content_type.into_media_type()),
                            context_url,
                            profile,
                            document,
                        ));
                    }
                    log::debug!("no valid media type found");
                    for link in response.headers().get_all(LINK).into_iter().flat_map(Link::parse_header) {
                        if link.rel() == Some(b"alternate") && link.type_() == Some(b"application/ld+json") {
                            log::debug!("link found");
                            if let Ok(resolved) = link.href().resolved(&url)
                                && let Ok(next) = IriBuf::try_from(resolved)
                            {
                                url = next;
                                redirection_number += 1;
                                continue 'next_url;
                            }
                        }
                    }

                    break Err(LoadError::new(url, Error::InvalidContentType));
                }
                code => break Err(LoadError::new(url, Error::QueryFailed(code))),
            }
        }
    }
}
