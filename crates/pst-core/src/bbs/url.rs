//! URL parsing for the boards we support.
//!
//! Both shitaraba and 2ch-compatible boards have several URL shapes
//! that point at the same logical resource. These helpers reduce all
//! of them to the canonical `(category|host, board, key?)` tuple.

use once_cell::sync::Lazy;
use regex::Regex;

use super::router::classify;
use super::types::BoardKind;
use crate::util::errors::{AppError, AppResult};

/// 任意の (スレ or 板) URL から、その板のトップ URL を正規化して返す。
/// Tauri command と pst-server の REST の双方から使う。
pub fn build_board_url(url: &str) -> AppResult<String> {
    let kind =
        classify(url).ok_or_else(|| AppError::InvalidUrl(format!("not a BBS URL: {url}")))?;
    match kind {
        BoardKind::Shitaraba => {
            let u = parse_shitaraba(url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a shitaraba URL: {url}")))?;
            Ok(format!("https://jbbs.shitaraba.net/{}/{}/", u.category, u.board_id))
        }
        BoardKind::Ch2Compat => {
            let u = parse_ch2(url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {url}")))?;
            Ok(format!("https://{}/{}/", u.host, u.board))
        }
    }
}

/// 板 URL (or スレ URL) とスレッド key から、その板の流儀に合った
/// canonical なスレッド URL を組み立てる。`${base}/${key}/` を素朴に
/// 繋ぐと 2ch 互換 (`/test/read.cgi/` が必要) で壊れるため、板種別ごと
/// に構築する。Tauri command と pst-server の REST の双方から使う。
pub fn build_thread_url(board_url: &str, key: &str) -> AppResult<String> {
    // key は数字のみ許可 (URL/パス注入対策)。
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::InvalidUrl(format!("invalid thread key: {key}")));
    }
    let kind = classify(board_url)
        .ok_or_else(|| AppError::InvalidUrl(format!("not a BBS URL: {board_url}")))?;
    match kind {
        BoardKind::Shitaraba => {
            let u = parse_shitaraba(board_url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a shitaraba URL: {board_url}")))?;
            Ok(format!(
                "https://jbbs.shitaraba.net/bbs/read.cgi/{}/{}/{}/",
                u.category, u.board_id, key
            ))
        }
        BoardKind::Ch2Compat => {
            let u = parse_ch2(board_url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {board_url}")))?;
            Ok(format!(
                "{}://{}/test/read.cgi/{}/{}/",
                u.scheme, u.host, u.board, key
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShitarabaUrl {
    pub category: String,
    pub board_id: String,
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ch2Url {
    /// URL scheme (`http` / `https`)。http 専用の互換板 (例:
    /// http://hibino.ddo.jp/bbs/peca/) があるので元 URL の scheme を保持し、
    /// subject.txt / dat / read.cgi の組み立てに使う (https 決め打ちは不可)。
    pub scheme: String,
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

// 2ch shapes (`board` は単一セグメントとは限らない: 一部の互換板は
//   http://host/bbs/peca/ のように board が複数セグメント、かつ http 専用。
//   そのため board は `.+`/`.+?` で多段許可し、scheme もキャプチャする):
//   {scheme}://{host}/{board}/
//   {scheme}://{host}/test/read.cgi/{board}/{key}/
//   {scheme}://{host}/{board}/dat/{key}.dat
//   {scheme}://{host}/test/bbs.cgi   (POST endpoint — no board in path)
static CH2_READ_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/]+)/test/read\.cgi/(?P<board>.+)/(?P<key>\d+)/?")
        .expect("ch2 read.cgi regex compiles")
});
static CH2_DAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/]+)/(?P<board>.+?)/dat/(?P<key>\d+)\.dat")
        .expect("ch2 dat regex compiles")
});
static CH2_BOARD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<scheme>https?)://(?P<host>[^/]+)/(?P<board>.+?)/?$").expect("ch2 board regex")
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
            scheme: c.name("scheme")?.as_str().to_string(),
            host: c.name("host")?.as_str().to_string(),
            board: c.name("board")?.as_str().to_string(),
            key: Some(c.name("key")?.as_str().to_string()),
        });
    }
    if let Some(c) = CH2_DAT_RE.captures(url) {
        return Some(Ch2Url {
            scheme: c.name("scheme")?.as_str().to_string(),
            host: c.name("host")?.as_str().to_string(),
            board: c.name("board")?.as_str().to_string(),
            key: Some(c.name("key")?.as_str().to_string()),
        });
    }
    if let Some(c) = CH2_BOARD_RE.captures(url) {
        return Some(Ch2Url {
            scheme: c.name("scheme")?.as_str().to_string(),
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
    fn shitaraba_read_cgi_drake_real_url() {
        // 実機 QA で詰まった実 URL (ドレイクch のコンタクト URL)。
        let u =
            parse_shitaraba("https://jbbs.shitaraba.net/bbs/read.cgi/internet/22667/1696385564/")
                .unwrap();
        assert_eq!(u.category, "internet");
        assert_eq!(u.board_id, "22667");
        assert_eq!(u.key.as_deref(), Some("1696385564"));
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

    #[test]
    fn shitaraba_read_cgi_with_l30_suffix() {
        // /l30 (last-N) や /501-1000 (range) のサフィックスはそのまま
        // ブラウザの read.cgi に渡されるが、内部表現としては cat/board/key
        // のみあれば十分なので、サフィックスを無視できることを確認する。
        let u =
            parse_shitaraba("https://jbbs.shitaraba.net/bbs/read.cgi/c/4567/1234567/l30").unwrap();
        assert_eq!(u.category, "c");
        assert_eq!(u.board_id, "4567");
        assert_eq!(u.key.as_deref(), Some("1234567"));
    }

    #[test]
    fn ch2_read_cgi_non_5ch_host() {
        // bbs.jpnkn.com や maguro.2ch.sc など、5ch 以外の 2ch 互換ホスト
        // でも host/board/key が抽出できることを確認 (ホスト名で絞らない)。
        let u = parse_ch2("https://example-bbs.invalid/test/read.cgi/myboard/1558097910/").unwrap();
        assert_eq!(u.host, "example-bbs.invalid");
        assert_eq!(u.board, "myboard");
        assert_eq!(u.key.as_deref(), Some("1558097910"));
    }

    #[test]
    fn ch2_http_multi_segment_board() {
        // 実 QA で詰まった http 専用 / board が複数セグメントの互換板
        // (http://hibino.ddo.jp/bbs/peca/)。scheme=http, board=bbs/peca。
        let u = parse_ch2("http://hibino.ddo.jp/bbs/peca/").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.host, "hibino.ddo.jp");
        assert_eq!(u.board, "bbs/peca");
        assert!(u.key.is_none());
    }

    #[test]
    fn ch2_http_multi_segment_dat() {
        let u = parse_ch2("http://hibino.ddo.jp/bbs/peca/dat/1781433331.dat").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.board, "bbs/peca");
        assert_eq!(u.key.as_deref(), Some("1781433331"));
    }

    #[test]
    fn ch2_read_cgi_multi_segment_board() {
        let u = parse_ch2("http://hibino.ddo.jp/test/read.cgi/bbs/peca/1781433331/").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.board, "bbs/peca");
        assert_eq!(u.key.as_deref(), Some("1781433331"));
    }

    #[test]
    fn ch2_scheme_and_single_segment_preserved() {
        // 単一セグメント板 + https がこれまで通り (回帰防止)。
        let u = parse_ch2("https://example.invalid/news4vip/").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.board, "news4vip");
    }
}
