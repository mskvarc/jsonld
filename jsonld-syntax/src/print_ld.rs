use crate::{Direction, LenientLangTag, LenientLangTagBuf, Nullable};
use iri_rs::IriRefBuf;
use jstrict::print::{Options, PrecomputeSize, Print, Size, printed_string_size, string_literal};
use std::fmt;

impl PrecomputeSize for Direction {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        Size::Width(printed_string_size(self.as_str()))
    }
}

impl Print for Direction {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        string_literal(self.as_str(), f)
    }
}

impl PrecomputeSize for LenientLangTagBuf {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        Size::Width(printed_string_size(self.as_str()))
    }
}

impl Print for LenientLangTagBuf {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        string_literal(self.as_str(), f)
    }
}

impl PrecomputeSize for LenientLangTag {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        Size::Width(printed_string_size(self.as_str()))
    }
}

impl Print for LenientLangTag {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        string_literal(self.as_str(), f)
    }
}

impl PrecomputeSize for Nullable<&IriRefBuf> {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        match self {
            Self::Null => Size::Width(4),
            Self::Some(v) => Size::Width(jstrict::print::printed_string_size(v.as_str())),
        }
    }
}

impl Print for Nullable<&IriRefBuf> {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(v) => string_literal(v.as_str(), f),
        }
    }
}

impl PrecomputeSize for Nullable<IriRefBuf> {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        match self {
            Self::Null => Size::Width(4),
            Self::Some(v) => Size::Width(printed_string_size(v.as_str())),
        }
    }
}

impl Print for Nullable<IriRefBuf> {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(v) => string_literal(v.as_str(), f),
        }
    }
}

impl PrecomputeSize for Nullable<bool> {
    fn pre_compute_size(&self, options: &Options, sizes: &mut Vec<Size>) -> Size {
        match self {
            Self::Null => Size::Width(4),
            Self::Some(b) => b.pre_compute_size(options, sizes),
        }
    }
}

impl Print for Nullable<bool> {
    fn fmt_with(&self, f: &mut fmt::Formatter, options: &Options, indent: usize) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(b) => b.fmt_with(f, options, indent),
        }
    }
}

impl PrecomputeSize for Nullable<&LenientLangTagBuf> {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        match self {
            Self::Null => Size::Width(4),
            Self::Some(v) => Size::Width(printed_string_size(v.as_str())),
        }
    }
}

impl Print for Nullable<&LenientLangTagBuf> {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(t) => string_literal(t.as_str(), f),
        }
    }
}

impl PrecomputeSize for Nullable<crate::Direction> {
    fn pre_compute_size(&self, _options: &Options, _sizes: &mut Vec<Size>) -> Size {
        match self {
            Self::Null => Size::Width(4),
            Self::Some(v) => Size::Width(printed_string_size(v.as_str())),
        }
    }
}

impl Print for Nullable<crate::Direction> {
    fn fmt_with(&self, f: &mut fmt::Formatter, _options: &Options, _indent: usize) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(d) => string_literal(d.as_str(), f),
        }
    }
}
