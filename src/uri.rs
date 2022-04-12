use std::io;

use http::{self, HeaderValue};
use hyper::Uri;

use crate::errors::Error;

pub(crate) trait UriExt {
    fn compute_redirect(&self, location: HeaderValue) -> Result<Uri, Error>;
    fn is_same_host(&self, other: &Uri) -> bool;
}

impl UriExt for Uri {
    fn compute_redirect(&self, location: HeaderValue) -> Result<Uri, Error> {
        let new_uri = http::Uri::from_maybe_shared(location)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        let old_uri = self.to_owned().into_parts();
        let old_parts = http::uri::Parts::from(old_uri);
        let mut new_parts = http::uri::Parts::from(new_uri);
        if new_parts.scheme.is_none() {
            new_parts.scheme = old_parts.scheme;
        }
        if new_parts.authority.is_none() {
            new_parts.authority = old_parts.authority;
        }
        let absolute_new_uri = http::Uri::from_parts(new_parts)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        Ok(absolute_new_uri.to_string().parse::<Uri>()?)
    }

    fn is_same_host(&self, other: &Uri) -> bool {
        self.host() == other.host() && self.port() == other.port()
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::{Uri, UriExt};

    #[test]
    fn extends_empty_path() {
        let base = "http://example.org".parse::<Uri>().unwrap();
        let location = "/index.html";
        let new = base
            .compute_redirect(HeaderValue::from_str(location).unwrap())
            .unwrap();
        assert_eq!("http://example.org/index.html", new.to_string());
    }

    #[test]
    fn retains_scheme_and_authority() {
        let base = "http://example.org/foo?x=1".parse::<Uri>().unwrap();
        let location = "/bar?y=1";
        let new = base
            .compute_redirect(HeaderValue::from_str(location).unwrap())
            .unwrap();
        assert_eq!("http://example.org/bar?y=1", new.to_string());
    }

    #[test]
    fn replaces_scheme_and_authority() {
        let base = "http://example.org/foo?x=1".parse::<Uri>().unwrap();
        let location = "https://example.com/bar?y=1";
        let new = base
            .compute_redirect(HeaderValue::from_str(location).unwrap())
            .unwrap();
        assert_eq!("https://example.com/bar?y=1", new.to_string());
    }
}
