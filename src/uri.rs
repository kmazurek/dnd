//! Conversion between local paths and `file://` URIs.
//!
//! Drag and drop transports files as URIs (RFC 8089). Decoding them here,
//! byte for byte, keeps file names exactly as the drag source sent them.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, percent_encode};

/// Characters escaped in the path component, besides controls and non-ASCII.
const PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Converts a `file://` URI into a local path.
///
/// Returns `None` for other schemes, for URIs naming a remote host and for
/// relative paths. Query strings and fragments are ignored.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let scheme_end = uri.find(':')?;
    if !uri[..scheme_end].eq_ignore_ascii_case("file") {
        return None;
    }
    let rest = &uri[scheme_end + 1..];
    let rest = rest.split(['?', '#']).next().unwrap_or_default();

    let path = match rest.strip_prefix("//") {
        Some(authority_and_path) => {
            let slash = authority_and_path.find('/')?;
            let host = &authority_and_path[..slash];
            if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
                return None;
            }
            &authority_and_path[slash..]
        }
        // `file:/path` is the minimal form allowed by RFC 8089.
        None => rest,
    };
    if !path.starts_with('/') {
        return None;
    }

    let bytes: Vec<u8> = percent_decode_str(path).collect();
    Some(PathBuf::from(OsStr::from_bytes(&bytes)))
}

/// Converts an absolute path into a `file://` URI.
pub fn path_to_file_uri(path: &Path) -> String {
    let encoded: String = path
        .as_os_str()
        .as_bytes()
        .split(|&b| b == b'/')
        .map(|segment| percent_encode(segment, PATH).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("file://{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_localhost_uris() {
        assert_eq!(
            file_uri_to_path("file:///tmp/a.txt"),
            Some("/tmp/a.txt".into())
        );
        assert_eq!(
            file_uri_to_path("file://localhost/tmp/a"),
            Some("/tmp/a".into())
        );
        assert_eq!(file_uri_to_path("FILE:///tmp/a"), Some("/tmp/a".into()));
        assert_eq!(file_uri_to_path("file:/tmp/a"), Some("/tmp/a".into()));
    }

    #[test]
    fn percent_escapes_are_decoded_byte_for_byte() {
        assert_eq!(
            file_uri_to_path("file:///a%20b/c%23d"),
            Some("/a b/c#d".into())
        );
        // Precomposed "ü" stays precomposed.
        assert_eq!(file_uri_to_path("file:///%C3%BC"), Some("/\u{fc}".into()));
        // Decomposed "u" + combining diaeresis stays decomposed.
        assert_eq!(
            file_uri_to_path("file:///u%CC%88"),
            Some("/u\u{308}".into())
        );
        // Not valid UTF-8, but a valid Unix file name.
        let raw = file_uri_to_path("file:///%FF").unwrap();
        assert_eq!(raw.as_os_str().as_bytes(), b"/\xff");
    }

    #[test]
    fn directories_keep_their_trailing_slash_semantics() {
        assert_eq!(
            file_uri_to_path("file:///tmp/dir/"),
            Some("/tmp/dir/".into())
        );
    }

    #[test]
    fn query_and_fragment_are_ignored() {
        assert_eq!(file_uri_to_path("file:///a?x=1#frag"), Some("/a".into()));
    }

    #[test]
    fn rejects_other_schemes_hosts_and_relative_paths() {
        assert_eq!(file_uri_to_path("https://example.com/a"), None);
        assert_eq!(file_uri_to_path("file://server/share/a"), None);
        assert_eq!(file_uri_to_path("file:relative"), None);
        assert_eq!(file_uri_to_path("/no/scheme"), None);
        assert_eq!(file_uri_to_path(""), None);
    }

    #[test]
    fn encoding_round_trips() {
        for path in ["/tmp/a b/c#d?.txt", "/ünïcødé ✓", "/100%/[x]", "/"] {
            let uri = path_to_file_uri(Path::new(path));
            assert!(uri.is_ascii(), "{uri}");
            assert_eq!(file_uri_to_path(&uri), Some(path.into()), "{uri}");
        }
        assert_eq!(path_to_file_uri(Path::new("/a b")), "file:///a%20b");
    }
}
