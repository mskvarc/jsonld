use super::{LD_JSON_MEDIA_TYPE, Loader, RemoteDocument};
use crate::LoadError;
use iri_rs::{Iri, IriBuf};
use jstrict::Parse;
use mediatype::MediaTypeBuf;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
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

fn ld_json_mime() -> MediaTypeBuf {
    static MIME: OnceLock<MediaTypeBuf> = OnceLock::new();
    MIME.get_or_init(|| LD_JSON_MEDIA_TYPE.into()).clone()
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
    #[must_use]
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
    ///
    /// The URL must match a mount point at a path boundary (mounting
    /// `http://example.com/a` does not capture `http://example.com/abc`), and
    /// the path is built only from single normal components: URLs containing
    /// `.` or `..` segments (or segments the platform would split further,
    /// like `\` on Windows) return `None`.
    ///
    /// This bounds the *constructed* path to the mounted directory. It does
    /// not follow the file system: a symbolic link inside the mount can still
    /// point outside it, so mount only directories whose contents you trust.
    #[must_use]
    pub fn filepath(&self, url: Iri<&str>) -> Option<PathBuf> {
        for (path, target_url) in &self.mount_points {
            if let Some(suffix) = url.as_str().strip_prefix(target_url.as_str()) {
                // Require a path boundary right after the matched prefix.
                if !(suffix.is_empty() || suffix.starts_with('/') || target_url.as_str().ends_with('/')) {
                    continue;
                }

                let mut filepath = path.clone();
                for seg in suffix.trim_start_matches('/').split('/') {
                    if seg.is_empty() {
                        continue;
                    }

                    // Only a single normal component may reach the file
                    // system: `.`, `..` or anything the platform parses as
                    // several components could escape the mount directory.
                    let mut components = Path::new(seg).components();
                    match (components.next(), components.next()) {
                        (Some(std::path::Component::Normal(_)), None) => filepath.push(seg),
                        _ => return None,
                    }
                }

                return Some(filepath);
            }
        }

        None
    }
}

impl Loader for FsLoader {
    type Error = Error;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        match self.filepath(url) {
            Some(filepath) => {
                let file = File::open(filepath).map_err(|e| LoadError::new(url.into(), Error::IO(e)))?;
                let mut buf_reader = BufReader::new(file);
                let mut contents = String::new();
                buf_reader.read_to_string(&mut contents).map_err(|e| LoadError::new(url.into(), Error::IO(e)))?;
                let (doc, _) = jstrict::Value::parse_str(&contents).map_err(|e| LoadError::new(url.into(), Error::Parse(e)))?;
                Ok(RemoteDocument::new(Some(url.into()), Some(ld_json_mime()), doc))
            }
            None => Err(LoadError::new(url.into(), Error::NoMountPoint)),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn loader() -> FsLoader {
        let mut loader = FsLoader::new();
        loader.mount(IriBuf::new("http://example.com/mount".to_string()).unwrap(), "/srv/data");
        loader
    }

    #[test]
    fn filepath_resolves_inside_mount() {
        assert_eq!(
            loader().filepath(Iri::parse("http://example.com/mount/a/b.json").unwrap()),
            Some(PathBuf::from("/srv/data/a/b.json"))
        );
    }

    #[test]
    fn filepath_rejects_parent_segments() {
        assert_eq!(loader().filepath(Iri::parse("http://example.com/mount/../../etc/secret").unwrap()), None);
        assert_eq!(loader().filepath(Iri::parse("http://example.com/mount/a/./b").unwrap()), None);
    }

    #[test]
    fn filepath_requires_prefix_boundary() {
        assert_eq!(loader().filepath(Iri::parse("http://example.com/mounted/a").unwrap()), None);
    }
}
