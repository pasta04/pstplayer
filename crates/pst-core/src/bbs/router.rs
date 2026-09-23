//! URL dispatch: figure out which `BoardKind` (and therefore which
//! `BoardClient` implementation) is responsible for a given URL.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::BoardKind;

static SHITARABA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https?://(?:[^.]+\.)?jbbs\.shitaraba\.net/").expect("shitaraba regex compiles")
});

static CH2_LIKE: Lazy<Regex> = Lazy::new(|| {
    // Matches /test/read.cgi/{board}/{key}/, /{board}/dat/{key}.dat, and the
    // bare board top URL. `board` は複数セグメントの場合がある
    // (http://host/bbs/peca/) ので read.cgi/dat の board は `.+`/`.+?` で許可。
    // bare board は「単一セグメント (任意で末尾 /)」か「末尾 / 付きの任意パス」
    // のみ board とみなす。末尾 / 無しの深いパス (例: /a/b/c/d) は対象外にし、
    // 非掲示板 URL を取り込みすぎないようにする (unknown_host_is_none 参照)。
    Regex::new(r"^https?://(?:[^/]+)/(?:test/read\.cgi/.+/\d+/?|.+/dat/\d+\.dat/?|[^/]+/?|.+/)$")
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
    fn shitaraba_read_cgi_with_l30_suffix_classifies() {
        // ブラウザがコピーする URL には /l30 や /501-1000 が付くことが
        // 多いので、それらが付いていても Shitaraba 判定できること。
        assert!(matches!(
            classify("https://jbbs.shitaraba.net/bbs/read.cgi/c/4567/1234567/l30"),
            Some(BoardKind::Shitaraba)
        ));
    }

    #[test]
    fn ch2_non_5ch_host_classifies() {
        // 5ch 以外の 2ch 互換ホスト (例: jpnkn 系) も Ch2Compat として扱う。
        assert!(matches!(
            classify("https://example-bbs.invalid/test/read.cgi/myboard/1558097910/"),
            Some(BoardKind::Ch2Compat)
        ));
    }

    #[test]
    fn unknown_host_is_none() {
        assert!(classify("https://www.example.com/random/path/with/extras").is_none());
    }

    #[test]
    fn ch2_http_multi_segment_board_classifies() {
        // 実 QA で詰まった http 専用 / board が複数セグメントの互換板。
        // 末尾 / 付きなので board とみなす。
        assert!(matches!(
            classify("http://hibino.ddo.jp/bbs/peca/"),
            Some(BoardKind::Ch2Compat)
        ));
    }
}
