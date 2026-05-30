//! URL parsing for the boards we support.
//!
//! Both shitaraba and 2ch-compatible boards have several URL shapes
//! that point at the same logical resource. These helpers reduce all
//! of them to the canonical `(category|host, board, key?)` tuple.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShitarabaUrl {
    pub category: String,
    pub board_id: String,
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ch2Url {
    pub host: String,
    pub board: String,
    pub key: Option<String>,
}

// shitaraba shapes:
//   https://jbbs.shitaraba.net/{cat}/{board}/
//   https://jbbs.shitaraba.net/bbs/read.cgi/{cat}/{board}/{key}/
//   https://jbbs.shitaraba.net/bbs/rawmode.cgi/{cat}/{board}/{key}/
//   https://jbbs.shitaraba.net/bbs/write.cgi/{cat}/{board}/{key}/
static SHITARABA_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^https?://jbbs\.shitaraba\.net/(?:bbs/(?:read|rawmode|write)\.cgi/)?(?P<cat>[^/]+)/(?P<board>[^/]+)(?:/(?P<key>\d+))?/?",
    )
    .expect("shitaraba URL regex compiles")
});

// 2ch shapes:
//   https://{host}/{board}/
//   https://{host}/test/read.cgi/{board}/{key}/
//   https://{host}/{board}/dat/{key}.dat
//   https://{host}/test/bbs.cgi   (POST endpoint — no board in path)
static CH2_READ_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https?://(?P<host>[^/]+)/test/read\.cgi/(?P<board>[^/]+)/(?P<key>\d+)/?")
        .expect("ch2 read.cgi regex compiles")
});
static CH2_DAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https?://(?P<host>[^/]+)/(?P<board>[^/]+)/dat/(?P<key>\d+)\.dat")
        .expect("ch2 dat regex compiles")
});
static CH2_BOARD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https?://(?P<host>[^/]+)/(?P<board>[^/]+)/?$").expect("ch2 board regex")
});

pub fn parse_shitaraba(url: &str) -> Option<ShitarabaUrl> {
    let c = SHITARABA_RE.captures(url)?;
    Some(ShitarabaUrl {
        category: c.name("cat")?.as_str().to_string(),
        board_id: c.name("board")?.as_str().to_string(),
        key: c.name("key").map(|m| m.as_str().to_string()),
    })
}

pub fn parse_ch2(url: &str) -> Option<Ch2Url> {
    if let Some(c) = CH2_READ_RE.captures(url) {
        return Some(Ch2Url {
            host: c.name("host")?.as_str().to_string(),
            board: c.name("board")?.as_str().to_string(),
            key: Some(c.name("key")?.as_str().to_string()),
        });
    }
    if let Some(c) = CH2_DAT_RE.captures(url) {
        return Some(Ch2Url {
            host: c.name("host")?.as_str().to_string(),
            board: c.name("board")?.as_str().to_string(),
            key: Some(c.name("key")?.as_str().to_string()),
        });
    }
    if let Some(c) = CH2_BOARD_RE.captures(url) {
        return Some(Ch2Url {
            host: c.name("host")?.as_str().to_string(),
            board: c.name("board")?.as_str().to_string(),
            key: None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shitaraba_board_only() {
        let u = parse_shitaraba("https://jbbs.shitaraba.net/computer/4567/").unwrap();
        assert_eq!(u.category, "computer");
        assert_eq!(u.board_id, "4567");
        assert!(u.key.is_none());
    }

    #[test]
    fn shitaraba_read_cgi() {
        let u = parse_shitaraba("https://jbbs.shitaraba.net/bbs/read.cgi/computer/4567/1234567/")
            .unwrap();
        assert_eq!(u.category, "computer");
        assert_eq!(u.board_id, "4567");
        assert_eq!(u.key.as_deref(), Some("1234567"));
    }

    #[test]
    fn shitaraba_rawmode() {
        let u = parse_shitaraba("https://jbbs.shitaraba.net/bbs/rawmode.cgi/c/1/2/").unwrap();
        assert_eq!(u.category, "c");
        assert_eq!(u.board_id, "1");
        assert_eq!(u.key.as_deref(), Some("2"));
    }

    #[test]
    fn ch2_read_cgi() {
        let u = parse_ch2("https://example.invalid/test/read.cgi/news4vip/1234567890/").unwrap();
        assert_eq!(u.host, "example.invalid");
        assert_eq!(u.board, "news4vip");
        assert_eq!(u.key.as_deref(), Some("1234567890"));
    }

    #[test]
    fn ch2_dat_url() {
        let u = parse_ch2("https://example.invalid/news4vip/dat/1234567890.dat").unwrap();
        assert_eq!(u.board, "news4vip");
        assert_eq!(u.key.as_deref(), Some("1234567890"));
    }

    #[test]
    fn ch2_board_only() {
        let u = parse_ch2("https://example.invalid/news4vip/").unwrap();
        assert_eq!(u.board, "news4vip");
        assert!(u.key.is_none());
    }
}
