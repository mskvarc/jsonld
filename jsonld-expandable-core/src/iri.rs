//! IRI / CURIE helpers used by the attribute parser.

/// Returns true if `s` looks like a full IRI with a scheme component
/// (matches `[a-zA-Z][a-zA-Z0-9+.-]*:`).
pub fn looks_like_iri(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
            return false;
        }
    }
    false
}

/// If `s` is a CURIE (`prefix:suffix`) whose `prefix` is in `prefixes`, expand
/// it to `<base><suffix>`. Otherwise return `s` unchanged.
///
/// IRIs that already have a scheme (i.e. `looks_like_iri` returns true on the
/// prefix portion) and the prefix is *not* in the table are returned as-is.
pub fn expand_curie(s: &str, prefixes: &[(String, String)]) -> String {
    if let Some((prefix, suffix)) = s.split_once(':') {
        for (name, base) in prefixes {
            if name == prefix {
                return format!("{base}{suffix}");
            }
        }
    }
    s.to_owned()
}
