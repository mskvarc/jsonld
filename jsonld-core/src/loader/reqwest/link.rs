use crate::HashMap;
use iri_rs::IriRefBuf;
use reqwest::header::HeaderValue;

pub struct Link {
    href: IriRefBuf,
    params: HashMap<Vec<u8>, Vec<u8>>,
}

impl Link {
    /// Parses every link value in the given `Link` header.
    ///
    /// A single header may carry several comma-separated link values
    /// (RFC 8288 §3): proxies and CDNs routinely coalesce multiple `Link`
    /// headers into one. Parameter values may be quoted or plain tokens.
    ///
    /// Parsing is best-effort: on malformed input the links parsed so far
    /// are returned and the rest of the header is ignored.
    pub fn parse_header(value: &HeaderValue) -> Vec<Self> {
        enum State {
            BeginHref,
            Href,
            NextParam,
            BeginKey,
            Key,
            BeginValue,
            QuotedValue,
            Value,
        }

        let mut state = State::BeginHref;
        let mut href = Vec::new();
        let mut current_key = Vec::new();
        let mut current_value = Vec::new();
        let mut params = HashMap::default();
        let mut links = Vec::new();

        let mut bytes = value.as_bytes().iter();

        macro_rules! store_param {
            () => {
                params.insert(std::mem::take(&mut current_key), std::mem::take(&mut current_value))
            };
        }

        macro_rules! finish_link {
            () => {
                if let Ok(href) = IriRefBuf::from_vec(std::mem::take(&mut href)) {
                    links.push(Self {
                        href,
                        params: std::mem::take(&mut params),
                    })
                }
            };
        }

        loop {
            match state {
                State::BeginHref => match bytes.next().copied() {
                    Some(b' ' | b',') => (),
                    Some(b'<') => state = State::Href,
                    _ => break,
                },
                State::Href => match bytes.next().copied() {
                    Some(b'>') => state = State::NextParam,
                    Some(b) => href.push(b),
                    None => break,
                },
                State::NextParam => match bytes.next().copied() {
                    Some(b' ') => (),
                    Some(b';') => state = State::BeginKey,
                    Some(b',') => {
                        finish_link!();
                        state = State::BeginHref;
                    }
                    Some(_) => break,
                    None => {
                        finish_link!();
                        break;
                    }
                },
                State::BeginKey => match bytes.next().copied() {
                    Some(b' ') => (),
                    Some(b',') => {
                        finish_link!();
                        state = State::BeginHref;
                    }
                    Some(b) => {
                        current_key.push(b);
                        state = State::Key;
                    }
                    None => {
                        finish_link!();
                        break;
                    }
                },
                State::Key => match bytes.next().copied() {
                    Some(b'=') => state = State::BeginValue,
                    Some(b';') => {
                        store_param!();
                        state = State::BeginKey;
                    }
                    Some(b',') => {
                        store_param!();
                        finish_link!();
                        state = State::BeginHref;
                    }
                    Some(b) => current_key.push(b),
                    None => {
                        store_param!();
                        finish_link!();
                        break;
                    }
                },
                State::BeginValue => match bytes.next().copied() {
                    Some(b'"') => state = State::QuotedValue,
                    Some(b) => {
                        current_value.push(b);
                        state = State::Value;
                    }
                    None => {
                        store_param!();
                        finish_link!();
                        break;
                    }
                },
                State::QuotedValue => match bytes.next().copied() {
                    Some(b'"') => {
                        store_param!();
                        state = State::NextParam;
                    }
                    // Quoted-pair escape (RFC 8288 uses HTTP quoted-string
                    // syntax).
                    Some(b'\\') => match bytes.next().copied() {
                        Some(b) => current_value.push(b),
                        None => break,
                    },
                    Some(b) => current_value.push(b),
                    None => break,
                },
                State::Value => match bytes.next().copied() {
                    Some(b';') => {
                        store_param!();
                        state = State::BeginKey;
                    }
                    Some(b',') => {
                        store_param!();
                        finish_link!();
                        state = State::BeginHref;
                    }
                    Some(b' ') => {
                        store_param!();
                        state = State::NextParam;
                    }
                    Some(b) => current_value.push(b),
                    None => {
                        store_param!();
                        finish_link!();
                        break;
                    }
                },
            }
        }

        links
    }

    pub fn href(&self) -> &IriRefBuf {
        &self.href
    }

    pub fn rel(&self) -> Option<&[u8]> {
        self.params.get(b"rel".as_slice()).map(Vec::as_slice)
    }

    pub fn type_(&self) -> Option<&[u8]> {
        self.params.get(b"type".as_slice()).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn parse_one(value: &str) -> Link {
        let mut links = Link::parse_header(&HeaderValue::from_str(value).unwrap());
        assert_eq!(links.len(), 1);
        links.pop().unwrap()
    }

    #[test]
    fn parse_link_1() {
        let link = parse_one("<http://www.example.org/context>; rel=\"context\"; type=\"application/ld+json\"");
        assert_eq!(link.href(), "http://www.example.org/context");
        assert_eq!(link.rel(), Some(b"context".as_slice()));
        assert_eq!(link.type_(), Some(b"application/ld+json".as_slice()));
    }

    #[test]
    fn parse_link_2() {
        let link = parse_one("<http://www.example.org/context>; rel=\"context\"; type=\"application/ld+json\"; foo=\"bar\"");
        assert_eq!(link.href(), "http://www.example.org/context");
        assert_eq!(link.rel(), Some(b"context".as_slice()));
        assert_eq!(link.type_(), Some(b"application/ld+json".as_slice()));
    }

    #[test]
    fn parse_link_3() {
        let link = parse_one("<http://www.example.org/context>");
        assert_eq!(link.href(), "http://www.example.org/context");
    }

    #[test]
    fn parse_link_unquoted_value() {
        let link = parse_one("<http://www.example.org/context>; rel=alternate; type=application/ld+json");
        assert_eq!(link.rel(), Some(b"alternate".as_slice()));
        assert_eq!(link.type_(), Some(b"application/ld+json".as_slice()));
    }

    #[test]
    fn parse_link_coalesced() {
        let links = Link::parse_header(
            &HeaderValue::from_str(
                "<http://www.example.org/a>; rel=\"alternate\", <http://www.example.org/context>; rel=\"http://www.w3.org/ns/json-ld#context\"",
            )
            .unwrap(),
        );
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].href(), "http://www.example.org/a");
        assert_eq!(links[0].rel(), Some(b"alternate".as_slice()));
        assert_eq!(links[1].href(), "http://www.example.org/context");
        assert_eq!(links[1].rel(), Some(b"http://www.w3.org/ns/json-ld#context".as_slice()));
    }

    #[test]
    fn parse_link_comma_inside_quoted_value() {
        let links = Link::parse_header(&HeaderValue::from_str("<http://www.example.org/a>; title=\"a, b\", <http://www.example.org/b>").unwrap());
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].params.get(b"title".as_slice()).map(Vec::as_slice), Some(b"a, b".as_slice()));
        assert_eq!(links[1].href(), "http://www.example.org/b");
    }

    #[test]
    fn parse_link_unquoted_value_before_comma() {
        let links = Link::parse_header(&HeaderValue::from_str("<http://www.example.org/a>; rel=alternate, <http://www.example.org/b>; rel=next").unwrap());
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].rel(), Some(b"alternate".as_slice()));
        assert_eq!(links[1].rel(), Some(b"next".as_slice()));
    }
}
