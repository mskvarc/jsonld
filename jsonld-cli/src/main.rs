use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use contextual::WithContext;
use iri_rs::IriBuf;
use jsonld::{JsonLdProcessor, Print, RemoteDocument, RemoteDocumentReference, syntax::Parse};
use rdf_rs::vocabulary::{IriIndex, IriVocabulary, IriVocabularyMut};

#[derive(Parser)]
#[command(name="json-ld", author, version, about, long_about = None)]
struct Args {
    /// Sets the level of verbosity.
    #[arg(short, long = "verbose", action = clap::ArgAction::Count)]
    verbosity: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Download the document behind the given URL.
    Fetch { url: IriBuf },

    /// Expand the given JSON-LD document.
    Expand {
        /// URL or file path of the document to expand.
        ///
        /// Of none, the standard input is used.
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
        no_vocab: bool,

        #[arg(long = "no-undef")]
        no_undef: bool,
    },

    Flatten {
        /// URL or file path of the document to flatten.
        ///
        /// Of none, the standard input is used.
        url_or_path: Option<IriOrPath>,

        /// Base URL to use when reading from the standard input or file system.
        #[arg(short, long)]
        base_url: Option<IriBuf>,
    },
}

#[derive(Clone)]
pub enum IriOrPath {
    Iri(IriBuf),
    Path(PathBuf),
}

impl FromStr for IriOrPath {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

    #[error("invalid mime: {0}")]
    Mime(#[from] mime::FromStrError),

    #[error("invalid blank id prefix: {0}")]
    BlankPrefix(#[from] rdf_rs::generator::InvalidBlankPrefix),

    #[error("loading failed: {0}")]
    Loading(#[from] jsonld::LoadError<ReqwestLoaderError>),

    #[error(transparent)]
    Expand(#[from] jsonld::ExpandError<ReqwestLoaderError>),

    #[error(transparent)]
    Compact(#[from] jsonld::CompactError<ReqwestLoaderError>),

    #[error(transparent)]
    Flatten(#[from] jsonld::FlattenError<IriIndex, rdf_rs::vocabulary::BlankIdIndex, ReqwestLoaderError>),

    #[error(transparent)]
    GeneratedId(#[from] jsonld::id::GeneratedIdError),
}

fn ld_json_mime() -> Result<mime::Mime, CliError> {
    "application/ld+json".parse().map_err(CliError::from)
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

    let mut vocabulary: rdf_rs::vocabulary::IndexVocabulary = rdf_rs::vocabulary::IndexVocabulary::new();
    let loader = jsonld::loader::ReqwestLoader::new();

    match args.command {
        Command::Fetch { url } => {
            let url = vocabulary.insert(url.as_ref());
            let remote_document = RemoteDocumentReference::iri(url).load_with(&mut vocabulary, &loader).await?;
            if let Some(remote_url) = remote_document.url() {
                if let Some(iri) = vocabulary.iri(remote_url) {
                    log::info!("document URL: {iri}");
                }
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
                let mut generator = rdf_rs::generator::Blank::new_with_prefix("b".to_string())?;

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
        Command::Flatten { url_or_path, base_url } => {
            let remote_document = get_remote_document(&mut vocabulary, url_or_path, base_url)?;

            let mut generator = rdf_rs::generator::Blank::new_with_prefix("b".to_string())?;

            let flattened = remote_document.flatten_with(&mut vocabulary, &mut generator, &loader).await?;
            println!("{}", flattened.with(&vocabulary).pretty_print());
        }
    }

    Ok(())
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
            Ok(RemoteDocumentReference::Loaded(RemoteDocument::new(url, Some(ld_json_mime()?), document)))
        }
        None => {
            let url = base_url.map(|iri| vocabulary.insert(iri.as_ref()));
            let content = std::io::read_to_string(std::io::stdin())?;
            let (document, _) = jsonld::syntax::Value::parse_str(&content)?;
            Ok(RemoteDocumentReference::Loaded(RemoteDocument::new(url, Some(ld_json_mime()?), document)))
        }
    }
}
