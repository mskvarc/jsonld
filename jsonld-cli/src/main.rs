//! Command line interface for the `jsonld` crate: fetch, expand, compact
//! and flatten JSON-LD documents from the shell.

use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use contextual::WithContext;
use iri_rs::IriBuf;
use jsonld::{JsonLdProcessor, LD_JSON_MEDIA_TYPE, Print, RemoteDocument, RemoteDocumentReference, syntax::Parse};
use rdfx::vocabulary::{IriIndex, IriVocabulary, IriVocabularyMut};

#[derive(Parser)]
#[command(name = "jsonld-cli", author, version, about, long_about = None)]
struct Args {
    /// Sets the level of verbosity.
    #[arg(short, long = "verbose", action = clap::ArgAction::Count)]
    verbosity: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
/// Sub-command the CLI was invoked with.
pub enum Command {
    /// Download the document behind the given URL.
    Fetch {
        /// URL of the document to fetch.
        url: IriBuf,
    },

    /// Expand the given JSON-LD document.
    Expand {
        /// URL or file path of the document to expand.
        ///
        /// If omitted, the document is read from the standard input.
        url_or_path: Option<IriOrPath>,

        /// Base URL to use when reading from the standard input or file system.
        #[arg(short, long)]
        base_url: Option<IriBuf>,

        /// Relabel the nodes.
        ///
        /// This will give a blank node identifier to unidentified nodes and
        /// replace existing blank node identifiers.
        #[arg(short = 'l', long)]
        relabel: bool,

        /// Put the expanded document in canonical form.
        #[arg(short, long)]
        canonicalize: bool,

        #[arg(long = "no-vocab")]
        /// Do not report terms expanded through `@vocab`.
        no_vocab: bool,

        #[arg(long = "no-undef")]
        /// Do not report undefined terms.
        no_undef: bool,
    },

    /// Compact the given JSON-LD document with a context.
    Compact {
        /// URL or file path of the JSON-LD context to compact with.
        context: IriOrPath,

        /// URL or file path of the document to compact.
        ///
        /// If omitted, the document is read from the standard input.
        url_or_path: Option<IriOrPath>,

        /// Base URL to use when reading from the standard input or file system.
        #[arg(short, long)]
        base_url: Option<IriBuf>,
    },

    /// Flatten a document into node objects.
    Flatten {
        /// URL or file path of the document to flatten.
        ///
        /// If omitted, the document is read from the standard input.
        url_or_path: Option<IriOrPath>,

        /// Base URL to use when reading from the standard input or file system.
        #[arg(short, long)]
        base_url: Option<IriBuf>,
    },
}

#[derive(Clone)]
/// Document location given either as an IRI or as a local path.
pub enum IriOrPath {
    /// An IRI.
    Iri(IriBuf),
    /// A path on the local file system.
    Path(PathBuf),
}

impl FromStr for IriOrPath {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // A single-letter "scheme" (`C:/foo`, `C:\foo`, `C:`) is a Windows
        // drive path, not an IRI: route it to the file-system branch instead
        // of parsing it as an IRI with scheme `c`.
        let bytes = s.as_bytes();
        let is_drive_path = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes.get(2), None | Some(b'\\' | b'/'));
        if is_drive_path {
            return Ok(Self::Path(s.into()));
        }

        match IriBuf::new(s.to_owned()) {
            Ok(iri) => Ok(Self::Iri(iri)),
            Err(e) => Ok(Self::Path(e.0.into())),
        }
    }
}

type ReqwestLoaderError = jsonld::loader::reqwest::Error;

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("logger init failed: {0}")]
    Logger(#[from] log::SetLoggerError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Parse(#[from] jstrict::parse::Error),

    #[error("invalid blank id prefix: {0}")]
    BlankPrefix(#[from] rdfx::generator::InvalidBlankPrefix),

    #[error("loading failed: {0}")]
    Loading(#[from] jsonld::LoadError<ReqwestLoaderError>),

    #[error(transparent)]
    Expand(#[from] jsonld::ExpandError<ReqwestLoaderError>),

    #[error("unable to extract JSON-LD context: {0}")]
    ContextExtraction(#[from] jsonld::ExtractContextError),

    #[error(transparent)]
    Compact(#[from] jsonld::CompactError<ReqwestLoaderError>),

    #[error(transparent)]
    Flatten(#[from] jsonld::FlattenError<IriIndex, rdfx::vocabulary::BlankIdIndex, ReqwestLoaderError>),

    #[error(transparent)]
    GeneratedId(#[from] jsonld::id::GeneratedIdError),
}

fn ld_json_mime() -> mediatype::MediaTypeBuf {
    LD_JSON_MEDIA_TYPE.into()
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    // Parse options.
    let args = Args::parse();

    // Init logger.
    stderrlog::new().verbosity(args.verbosity as usize).init()?;

    let mut vocabulary: rdfx::vocabulary::IndexVocabulary = rdfx::vocabulary::IndexVocabulary::new();
    let loader = jsonld::loader::ReqwestLoader::new();

    match args.command {
        Command::Fetch { url } => {
            let url = vocabulary.insert(url.as_ref());
            let remote_document = RemoteDocumentReference::iri(url).load_with(&mut vocabulary, &loader).await?;
            if let Some(remote_url) = remote_document.url()
                && let Some(iri) = vocabulary.iri(remote_url)
            {
                log::info!("document URL: {iri}");
            }

            println!("{}", remote_document.document().pretty_print());
        }
        Command::Expand {
            url_or_path,
            base_url,
            relabel,
            canonicalize,
            no_vocab,
            no_undef,
        } => {
            let remote_document = get_remote_document(&mut vocabulary, url_or_path, base_url)?;

            let options = jsonld::Options {
                expansion_policy: jsonld::expansion::Policy {
                    invalid: jsonld::expansion::Action::Reject,
                    vocab: if no_vocab {
                        jsonld::expansion::Action::Reject
                    } else {
                        jsonld::expansion::Action::Keep
                    },
                    allow_undefined: !no_undef,
                },
                ..Default::default()
            };

            let mut expanded = remote_document.expand_with_using(&mut vocabulary, &loader, options).await?;

            if relabel {
                let mut generator = rdfx::generator::Blank::new_with_prefix("b".to_string())?;

                if canonicalize {
                    expanded.relabel_and_canonicalize_with(&mut vocabulary, &mut generator)?;
                } else {
                    expanded.relabel_with(&mut vocabulary, &mut generator)?;
                }
            } else if canonicalize {
                expanded.canonicalize();
            }

            println!("{}", expanded.with(&vocabulary).pretty_print());
        }
        Command::Compact {
            context,
            url_or_path,
            base_url,
        } => {
            let remote_document = get_remote_document(&mut vocabulary, url_or_path, base_url.clone())?;
            let context = get_remote_context(&mut vocabulary, context, base_url)?;

            let compacted = remote_document.compact_with(&mut vocabulary, context, &loader).await?;
            println!("{}", compacted.pretty_print());
        }
        Command::Flatten { url_or_path, base_url } => {
            let remote_document = get_remote_document(&mut vocabulary, url_or_path, base_url)?;

            let mut generator = rdfx::generator::Blank::new_with_prefix("b".to_string())?;

            let flattened = remote_document.flatten_with(&mut vocabulary, &mut generator, &loader).await?;
            println!("{}", flattened.with(&vocabulary).pretty_print());
        }
    }

    Ok(())
}

fn get_remote_context(
    vocabulary: &mut impl IriVocabularyMut<Iri = IriIndex>,
    url_or_path: IriOrPath,
    base_url: Option<IriBuf>,
) -> Result<jsonld::RemoteContextReference<IriIndex>, CliError> {
    match url_or_path {
        IriOrPath::Iri(url) => {
            let url = vocabulary.insert(url.as_ref());
            Ok(jsonld::RemoteContextReference::iri(url))
        }
        IriOrPath::Path(path) => {
            use jsonld::ExtractContext;
            let url = base_url.map(|iri| vocabulary.insert(iri.as_ref()));
            let text = std::fs::read_to_string(path)?;
            let (document, _) = jsonld::syntax::Value::parse_str(&text)?;
            let context = document.into_ld_context()?;
            Ok(jsonld::RemoteContextReference::Loaded(RemoteDocument::new(url, Some(ld_json_mime()), context)))
        }
    }
}

fn get_remote_document(
    vocabulary: &mut impl IriVocabularyMut<Iri = IriIndex>,
    url_or_path: Option<IriOrPath>,
    base_url: Option<IriBuf>,
) -> Result<RemoteDocumentReference<IriIndex>, CliError> {
    match url_or_path {
        Some(IriOrPath::Iri(url)) => {
            let url = vocabulary.insert(url.as_ref());
            Ok(RemoteDocumentReference::iri(url))
        }
        Some(IriOrPath::Path(path)) => {
            let url = base_url.map(|iri| vocabulary.insert(iri.as_ref()));
            let content = std::fs::read_to_string(path)?;
            let (document, _) = jsonld::syntax::Value::parse_str(&content)?;
            Ok(RemoteDocumentReference::Loaded(RemoteDocument::new(url, Some(ld_json_mime()), document)))
        }
        None => {
            let url = base_url.map(|iri| vocabulary.insert(iri.as_ref()));
            let content = std::io::read_to_string(std::io::stdin())?;
            let (document, _) = jsonld::syntax::Value::parse_str(&content)?;
            Ok(RemoteDocumentReference::Loaded(RemoteDocument::new(url, Some(ld_json_mime()), document)))
        }
    }
}
