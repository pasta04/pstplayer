//! 2ch-compatible (5ch, jpnkn, …) client. See `docs/protocols/bbs.md` §4.

use super::{
    encoding::{percent_encode, BoardEncoding},
    parse::{parse_ch2_dat, parse_ch2_subject},
    post_result::{classify_ch2, PostOutcome},
    types::{FetchState, Post, PostRequest},
    url::parse_ch2,
};
use crate::util::{
    errors::{AppError, AppResult},
    http::cookie_client,
};

pub struct Ch2Client {
    http: reqwest::Client,
}

impl Default for Ch2Client {
    fn default() -> Self {
        Self::new()
    }
}

/// 増分取得 (Range: bytes={last_byte-1}-) の応答本文を検証する。
/// 先頭バイトが直前回の終端 (\n) と一致するなら残りが新着、そうで
/// なければ dat が再構築されている。
enum OverlapCheck<'a> {
    /// 継続している。中身は先頭の \n を除いた新着バイト列 (空 = 新着なし)。
    Continued(&'a [u8]),
    /// 繋がらない (削除 / 編集で dat が変わった)。全件取り直しが必要。
    Rebuilt,
}

fn split_overlap(bytes: &[u8]) -> OverlapCheck<'_> {
    match bytes.first() {
        Some(b'\n') => OverlapCheck::Continued(&bytes[1..]),
        Some(_) => OverlapCheck::Rebuilt,
        // 0 バイト応答は「新着なし」とみなす (通常は 1 バイト以上返る)。
        None => OverlapCheck::Continued(bytes),
    }
}

impl Ch2Client {
    pub fn new() -> Self {
        Self {
            http: cookie_client(),
        }
    }

    fn subject_url(scheme: &str, host: &str, board: &str) -> String {
        format!("{scheme}://{host}/{board}/subject.txt")
    }

    fn dat_url(scheme: &str, host: &str, board: &str, key: &str) -> String {
        format!("{scheme}://{host}/{board}/dat/{key}.dat")
    }

    fn write_url(scheme: &str, host: &str) -> String {
        format!("{scheme}://{host}/test/bbs.cgi")
    }

    fn setting_url(scheme: &str, host: &str, board: &str) -> String {
        format!("{scheme}://{host}/{board}/SETTING.TXT")
    }

    /// 板の `SETTING.TXT` (Shift_JIS) を取得して最大レス数等を返す。
    /// 2ch 互換は `BBS_RES_MAX` が最大レス数。
    pub async fn fetch_setting(&self, board_url: &str) -> AppResult<super::types::BoardSetting> {
        let u = parse_ch2(board_url)
            .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {board_url}")))?;
        let url = Self::setting_url(&u.scheme, &u.host, &u.board);
        let bytes = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let body = BoardEncoding::ShiftJis.decode(&bytes)?;
        Ok(super::parse::parse_board_setting(&body))
    }

    /// Fetch and parse `subject.txt` (Shift_JIS).
    pub async fn list_threads(
        &self,
        board_url: &str,
    ) -> AppResult<Vec<super::parse::SubjectEntry>> {
        let u = parse_ch2(board_url)
            .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {board_url}")))?;
        let url = Self::subject_url(&u.scheme, &u.host, &u.board);
        let bytes = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let body = BoardEncoding::ShiftJis.decode(&bytes)?;
        Ok(parse_ch2_subject(&body))
    }

    /// Fetch a thread dat with incremental `Range` support.
    pub async fn fetch_thread(
        &self,
        thread_url: &str,
        prev: Option<&FetchState>,
    ) -> AppResult<(Vec<Post>, FetchState)> {
        let u = parse_ch2(thread_url)
            .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {thread_url}")))?;
        let key = u
            .key
            .as_deref()
            .ok_or_else(|| AppError::InvalidUrl(format!("missing thread key in: {thread_url}")))?;

        let url = Self::dat_url(&u.scheme, &u.host, &u.board, key);
        let prev_count = prev.map(|p| p.last_count).unwrap_or(0);
        let prev_byte = prev.map(|p| p.last_byte).unwrap_or(0);
        // 増分は「既知の最終バイト (直前の \n) を 1 バイト含めて」要求し、
        // 応答の先頭が \n であることを検証する 2ch クライアントの定石を使う。
        // - 新着なしでも Range が常に満たせるため 206 (1 バイト) になる。
        //   起点 == サイズ の Range に対し、If-Modified-Since が一致していても
        //   304 でなく 416 を返す Apache が実在する (komokomo.ddns.net で実測)。
        //   その 416 → 全件再取得が毎回走ると、フロントに全レスが増分として
        //   届いてしまう。
        // - 先頭が \n でなければ dat が再構築された (削除等) と検知できる。
        let overlap = prev_byte > 0;
        let mut req = self.http.get(&url);
        if let Some(p) = prev {
            if let Some(lm) = &p.last_modified {
                req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
            }
            if overlap {
                req = req.header(reqwest::header::RANGE, format!("bytes={}-", prev_byte - 1));
            }
        }
        let resp = req.send().await?;
        let status = resp.status();

        // 304 Not Modified ⇒ no update.
        if status == reqwest::StatusCode::NOT_MODIFIED {
            let mut state = prev.cloned().unwrap_or_default();
            state.full_reload = false;
            return Ok((Vec::new(), state));
        }
        // 416 Range Not Satisfiable ⇒ thread shrank or was rebuilt; reset.
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Box::pin(self.fetch_thread(thread_url, None)).await;
        }
        if !status.is_success() {
            return Err(AppError::Network(format!("dat returned {status}")));
        }

        let lm = resp
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().await?;
        let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;

        if is_partial && overlap {
            match split_overlap(&bytes) {
                OverlapCheck::Rebuilt => {
                    // 先頭が \n でない = 手元の範囲と繋がらない。全件取り直す。
                    return Box::pin(self.fetch_thread(thread_url, None)).await;
                }
                OverlapCheck::Continued(new_bytes) => {
                    if new_bytes.is_empty() {
                        let state = FetchState {
                            last_modified: lm,
                            last_byte: prev_byte,
                            last_count: prev_count,
                            full_reload: false,
                        };
                        return Ok((Vec::new(), state));
                    }
                    let added_bytes = new_bytes.len() as u64;
                    let body = BoardEncoding::ShiftJis.decode(new_bytes)?;
                    // 増分は 1 から番号が振り直されるので継ぎ足す。
                    let posts: Vec<Post> = parse_ch2_dat(&body)
                        .into_iter()
                        .map(|mut p| {
                            p.number = p.number.saturating_add(prev_count);
                            p
                        })
                        .collect();
                    let state = FetchState {
                        last_modified: lm,
                        last_byte: prev_byte + added_bytes,
                        last_count: prev_count + posts.len() as u32,
                        full_reload: false,
                    };
                    return Ok((posts, state));
                }
            }
        }

        // 200 (初回 / サーバが Range 無視) = スレ全体のスナップショット。
        // full_reload でフロントに「置換」を指示する。
        let added_bytes = bytes.len() as u64;
        let body = BoardEncoding::ShiftJis.decode(&bytes)?;
        let posts = parse_ch2_dat(&body);
        let state = FetchState {
            last_modified: lm,
            last_byte: added_bytes,
            last_count: posts.len() as u32,
            full_reload: true,
        };
        Ok((posts, state))
    }

    /// Post via `bbs.cgi`. Two-stage confirmation (cookie round-trip)
    /// is handled implicitly because we share a cookie store.
    pub async fn post(&self, thread_url: &str, req: &PostRequest) -> AppResult<()> {
        let u = parse_ch2(thread_url)
            .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {thread_url}")))?;
        let key = u
            .key
            .as_deref()
            .ok_or_else(|| AppError::InvalidUrl(format!("missing thread key in: {thread_url}")))?;
        let url = Self::write_url(&u.scheme, &u.host);
        let referer = format!("https://{}/{}/", u.host, u.board);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let enc = |s: &str| percent_encode(&BoardEncoding::ShiftJis.encode(s));
        let body_str = format!(
            "bbs={}&key={}&FROM={}&mail={}&MESSAGE={}&time={}&submit={}",
            enc(&u.board),
            enc(key),
            enc(&req.name),
            enc(&req.mail),
            enc(&req.body),
            time,
            enc("書き込む"),
        );

        // First POST — may return a cookie confirmation page.
        let resp1 = self
            .http
            .post(&url)
            .header("Referer", &referer)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body_str.clone())
            .send()
            .await?;
        let bytes1 = resp1.bytes().await?;
        let text1 = BoardEncoding::ShiftJis
            .decode(&bytes1)
            .unwrap_or_else(|_| String::new());
        match classify_ch2(&text1) {
            PostOutcome::Success => Ok(()),
            PostOutcome::Rejected(kind) => Err(kind.into_error()),
            PostOutcome::NeedsCookieConfirm => {
                // Re-send with the cookies the server just handed us.
                let resp2 = self
                    .http
                    .post(&url)
                    .header("Referer", &referer)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(body_str)
                    .send()
                    .await?;
                let bytes2 = resp2.bytes().await?;
                let text2 = BoardEncoding::ShiftJis
                    .decode(&bytes2)
                    .unwrap_or_else(|_| String::new());
                match classify_ch2(&text2) {
                    PostOutcome::Success => Ok(()),
                    PostOutcome::Rejected(kind) => Err(kind.into_error()),
                    PostOutcome::NeedsCookieConfirm => Err(AppError::PostRejected(
                        "Cookie 確認の再送でも投稿が通りませんでした".into(),
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_builders() {
        assert_eq!(
            Ch2Client::subject_url("https", "example.invalid", "news4vip"),
            "https://example.invalid/news4vip/subject.txt"
        );
        assert_eq!(
            Ch2Client::dat_url("https", "example.invalid", "news4vip", "1234567890"),
            "https://example.invalid/news4vip/dat/1234567890.dat"
        );
        assert_eq!(
            Ch2Client::write_url("https", "example.invalid"),
            "https://example.invalid/test/bbs.cgi"
        );
    }

    #[test]
    fn url_builders_http_multi_segment_board() {
        // 実 QA で詰まった http 専用 / board が複数セグメントの互換板
        // (http://hibino.ddo.jp/bbs/peca/)。scheme と board をそのまま使う。
        assert_eq!(
            Ch2Client::subject_url("http", "hibino.ddo.jp", "bbs/peca"),
            "http://hibino.ddo.jp/bbs/peca/subject.txt"
        );
        assert_eq!(
            Ch2Client::dat_url("http", "hibino.ddo.jp", "bbs/peca", "1781433331"),
            "http://hibino.ddo.jp/bbs/peca/dat/1781433331.dat"
        );
        assert_eq!(
            Ch2Client::write_url("http", "hibino.ddo.jp"),
            "http://hibino.ddo.jp/test/bbs.cgi"
        );
    }
}

#[cfg(test)]
mod overlap_tests {
    use super::*;

    #[test]
    fn continued_with_new_data() {
        let b = b"\nfoo<>bar\n";
        match split_overlap(b) {
            OverlapCheck::Continued(rest) => assert_eq!(rest, b"foo<>bar\n"),
            OverlapCheck::Rebuilt => panic!("should continue"),
        }
    }

    #[test]
    fn continued_no_new_data() {
        match split_overlap(b"\n") {
            OverlapCheck::Continued(rest) => assert!(rest.is_empty()),
            OverlapCheck::Rebuilt => panic!("should continue"),
        }
    }

    #[test]
    fn rebuilt_when_first_byte_differs() {
        assert!(matches!(split_overlap(b"abc"), OverlapCheck::Rebuilt));
    }
}
