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
        let mut req = self.http.get(&url);
        if let Some(p) = prev {
            if let Some(lm) = &p.last_modified {
                req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
            }
            if p.last_byte > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={}-", p.last_byte));
            }
        }
        let resp = req.send().await?;
        let status = resp.status();

        // 304 Not Modified ⇒ no update.
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok((Vec::new(), prev.cloned().unwrap_or_default()));
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
        let added_bytes = bytes.len() as u64;
        let body = BoardEncoding::ShiftJis.decode(&bytes)?;

        let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let prev_count = prev.map(|p| p.last_count).unwrap_or(0);
        let prev_byte = prev.map(|p| p.last_byte).unwrap_or(0);

        // For incremental fetches we have to re-number from where we left off.
        let raw_posts = parse_ch2_dat(&body);
        let posts: Vec<Post> = if is_partial {
            raw_posts
                .into_iter()
                .map(|mut p| {
                    p.number = p.number.saturating_add(prev_count);
                    p
                })
                .collect()
        } else {
            raw_posts
        };

        let last_count = if is_partial {
            prev_count + posts.len() as u32
        } else {
            posts.len() as u32
        };
        let last_byte = if is_partial {
            prev_byte + added_bytes
        } else {
            added_bytes
        };
        let state = FetchState {
            last_modified: lm,
            last_byte,
            last_count,
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
