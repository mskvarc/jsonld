use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
};

/// Folds an ASCII uppercase byte to its lowercase counterpart, leaving every
/// other byte untouched.
///
/// Used to compare language tags case-insensitively. Well-formed tags are
/// ASCII-only, so folding bytes rather than characters is enough; non-ASCII
/// bytes, which only an ill-formed tag can contain, are compared as they are.
pub fn into_smallcase(c: u8) -> u8 {
    if c.is_ascii_uppercase() { c + 0x20 } else { c }
}

pub fn case_insensitive_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() == b.len() {
        for i in 0..a.len() {
            if into_smallcase(a[i]) != into_smallcase(b[i]) {
                return false;
            }
        }

        true
    } else {
        false
    }
}

pub fn case_insensitive_hash<H: Hasher>(bytes: &[u8], hasher: &mut H) {
    for b in bytes {
        into_smallcase(*b).hash(hasher);
    }
}

pub fn case_insensitive_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let mut i = 0;

    loop {
        if a.len() <= i {
            if b.len() <= i {
                return Ordering::Equal;
            }

            // `a` is a strict prefix of `b`: shorter sorts first, matching
            // lexicographic byte order.
            return Ordering::Less;
        } else if b.len() <= i {
            return Ordering::Greater;
        }
        match into_smallcase(a[i]).cmp(&into_smallcase(b[i])) {
            Ordering::Equal => i += 1,
            ord => return ord,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strict prefix sorts before its extension, matching `str`'s
    /// lexicographic order (modulo case).
    #[test]
    fn prefix_sorts_first() {
        assert_eq!(case_insensitive_cmp(b"en", b"en-US"), Ordering::Less);
        assert_eq!(case_insensitive_cmp(b"en-US", b"en"), Ordering::Greater);
        assert_eq!(case_insensitive_cmp(b"en-us", b"EN-US"), Ordering::Equal);
        assert_eq!(case_insensitive_cmp(b"de", b"en"), Ordering::Less);
    }
}
