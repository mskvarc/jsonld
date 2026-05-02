use super::{Loader, RemoteDocument};
use crate::{LoadError, LoadingResult};
use iri_rs::{Iri, IriBuf};
use jstrict::Parse;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

/// Loading error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No mount point found for the given IRI.
    #[error("no mount point")]
    NoMountPoint,

    /// IO error.
    #[error("IO: {0}")]
    IO(std::io::Error),

    /// Parse error.
    #[error("parse error: {0}")]
    Parse(jstrict::parse::Error),
}

/// File-system loader.
///
/// This is a special JSON-LD document loader that can load document from the file system by
/// attaching a directory to specific URLs.
///
/// Loaded documents are not cached: a new file system read is made each time
/// an URL is loaded even if it has already been queried before.
#[derive(Default)]
pub struct FsLoader {
    mount_points: Vec<(PathBuf, IriBuf)>,
}

impl FsLoader {
    /// Creates a new file system loader with the given content `parser`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind the given IRI prefix to the given path.
    ///
    /// Any document with an IRI matching the given prefix will be loaded from
    /// the referenced local directory.
    #[inline(always)]
    pub fn mount<P: AsRef<Path>>(&mut self, url: IriBuf, path: P) {
        self.mount_points.push((path.as_ref().into(), url));
    }

    /// Returns the local file path associated to the given `url` if any.
    pub fn filepath(&self, url: Iri<&str>) -> Option<PathBuf> {
        for (path, target_url) in &self.mount_points {
            if let Some(suffix) = url.as_str().strip_prefix(target_url.as_str()) {
                let mut filepath = path.clone();
                for seg in suffix.trim_start_matches('/').split('/') {
                    if !seg.is_empty() {
                        filepath.push(seg)
                    }
                }

                return Some(filepath);
            }
        }

        None
    }
}

impl Loader for FsLoader {
    async fn load(&self, url: Iri<&str>) -> LoadingResult<IriBuf> {
        match self.filepath(url) {
            Some(filepath) => {
                let file = File::open(filepath).map_err(|e| LoadError::new(url.into(), Error::IO(e)))?;
                let mut buf_reader = BufReader::new(file);
                let mut contents = String::new();
                buf_reader.read_to_string(&mut contents).map_err(|e| LoadError::new(url.into(), Error::IO(e)))?;
                let (doc, _) = jstrict::Value::parse_str(&contents).map_err(|e| LoadError::new(url.into(), Error::Parse(e)))?;
                Ok(RemoteDocument::new(Some(url.into()), Some("application/ld+json".parse().unwrap()), doc))
            }
            None => Err(LoadError::new(url.into(), Error::NoMountPoint)),
        }
    }
}
