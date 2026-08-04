use iri_rs::{IriRef, IriRefBuf};

/// Error raised when a string is not a compact IRI.
pub struct InvalidCompactIri<T>(pub T);

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
/// Compact IRI: a prefix and a suffix separated by a colon.
// `repr(transparent)` guarantees the layout assumed by the `&str` transmute
// in `new_unchecked`.
#[repr(transparent)]
pub struct CompactIri(str);

impl CompactIri {
    /// Borrows the given string as a compact IRI, checking that it is one.
    pub fn new(s: &str) -> Result<&Self, InvalidCompactIri<&str>> {
        match s.split_once(':') {
            Some((prefix, suffix)) if prefix != "_" && !suffix.starts_with("//") => match IriRef::parse(s) {
                Ok(_) => Ok(unsafe { Self::new_unchecked(s) }),
                Err(_) => Err(InvalidCompactIri(s)),
            },
            _ => Err(InvalidCompactIri(s)),
        }
    }

    /// Creates a new compact IRI without parsing it.
    ///
    /// # Safety
    ///
    /// `s` must be a valid compact IRI: it must contain a `:`, the segment
    /// before `:` must not equal `_`, the segment after `:` must not start with
    /// `//`, and `s` as a whole must parse as an `IriRef`.
    pub unsafe fn new_unchecked(s: &str) -> &Self {
        unsafe { std::mem::transmute(s) }
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Clones this `CompactIri` into an owned one.
    pub fn to_owned(&self) -> CompactIriBuf {
        CompactIriBuf(self.0.to_owned())
    }

    /// Returns the part before the colon, which names a term to expand.
    pub fn prefix(&self) -> &str {
        // SAFETY: a `CompactIri` always contains a `:` (enforced by `new` /
        // `new_unchecked`).
        let i = unsafe { self.find(':').unwrap_unchecked() };
        &self[0..i]
    }

    /// Returns the part after the colon, appended to the expanded prefix.
    pub fn suffix(&self) -> &str {
        // SAFETY: see `prefix`.
        let i = unsafe { self.find(':').unwrap_unchecked() };
        &self[i + 1..]
    }

    /// Returns this compact IRI as an IRI reference.
    ///
    /// Every compact IRI is also a syntactically valid IRI reference; that is
    /// checked when the value is created.
    pub fn as_iri_ref(&self) -> IriRef<&str> {
        // SAFETY: validated as an `IriRef` at construction.
        unsafe { IriRef::parse(self.as_str()).unwrap_unchecked() }
    }
}

impl std::ops::Deref for CompactIri {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for CompactIri {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CompactIri {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
/// Owned compact IRI.
pub struct CompactIriBuf(String);

impl CompactIriBuf {
    /// Takes ownership of the given string, checking that it is a compact IRI.
    pub fn new(s: String) -> Result<Self, InvalidCompactIri<String>> {
        match CompactIri::new(&s) {
            Ok(_) => Ok(unsafe { Self::new_unchecked(s) }),
            Err(_) => Err(InvalidCompactIri(s)),
        }
    }

    /// Creates a new compact IRI without parsing it.
    ///
    /// # Safety
    ///
    /// `s` must satisfy the contract of [`CompactIri::new_unchecked`].
    pub unsafe fn new_unchecked(s: String) -> Self {
        Self(s)
    }

    /// Borrows this value as a [`CompactIri`].
    pub fn as_compact_iri(&self) -> &CompactIri {
        unsafe { CompactIri::new_unchecked(&self.0) }
    }

    /// Converts this compact IRI into an owned IRI reference, without
    /// reallocating.
    pub fn into_iri_ref(self) -> IriRefBuf {
        // SAFETY: validated as an `IriRef` at construction.
        unsafe { IriRefBuf::new(self.0).unwrap_unchecked() }
    }

    /// Unwraps the underlying string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::borrow::Borrow<CompactIri> for CompactIriBuf {
    fn borrow(&self) -> &CompactIri {
        self.as_compact_iri()
    }
}

impl std::ops::Deref for CompactIriBuf {
    type Target = CompactIri;

    fn deref(&self) -> &CompactIri {
        self.as_compact_iri()
    }
}
