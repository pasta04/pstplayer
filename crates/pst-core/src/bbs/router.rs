//! URL dispatch: figure out which `BoardKind` (and therefore which
//! `BoardClient` implementation) is responsible for a given URL.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::BoardKind;

static SHITARABA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https?://(?:[^.]+\.)?jbbs\.shitaraba\.net/").expect("shitaraba regex compiles")
});

static CH2_LIKE: Lazy<Regex> = Lazy::new(|| {
    // Matches /test/read.cgi/{board}/{key}/, /{board}/dat/{key}.dat,
    // and the bare board top URL. We err on the side of including
    // common 2ch-compatible hosts.
    Regex::new(r"^https?://(?:[^/]+)/(?:test/read\.cgi/[^/]+/\d+/?|[^/]+/dat/\d+\.dat/?|[^/]+/?)$")
        .expect("ch2-like regex compiles")
});

pub fn classify(url: &str) -> Option<BoardKind> {
    if SHITARABA.is_match(url) {
        return Some(BoardKind::Shitaraba);
    }
    if CH2_LIKE.is_match(url) {
        return Some(BoardKind::Ch2Compat);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shitaraba_host() {
        assert!(matches!(
            classify("https://jbbs.shitaraba.net/computer/4567/1234/"),
            Some(BoardKind::Shitaraba)
        ));
        assert!(matches!(
            classify("https://jbbs.shitaraba.net/bbs/read.cgi/computer/4567/1234/"),
            Some(BoardKind::Shitaraba)
        ));
    }

    #[test]
    fn ch2_read_cgi() {
        assert!(matches!(
            classify("https://example.invalid/test/read.cgi/board/1234/"),
            Some(BoardKind::Ch2Compat)
        ));
    }

    #[test]
    fn ch2_dat_url() {
        assert!(matches!(
            classify("https://example.invalid/board/dat/1234.dat"),
            Some(BoardKind::Ch2Compat)
        ));
    }

    #[test]
    fn unknown_host_is_none() {
        assert!(classify("https://www.example.com/random/path/with/extras").is_none());
    }
}
