//! Shitaraba JBBS client. See `docs/protocols/bbs.md` §3.

use serde::Serialize;

use super::{
    encoding::{percent_encode, BoardEncoding},
    parse::{parse_shitaraba_dat, parse_shitaraba_subject},
    post_result::{classify_shitaraba, PostOutcome},
    types::{FetchState, Post, PostRequest},
    url::parse_shitaraba,
};
use crate::util::{
    errors::{AppError, AppResult},
    http::cookie_client,
};

#[derive(Debug, Clone, Serialize)]
pub struct BoardLocation {
    pub category: String,
    pub board_id: String,
    pub key: Option<String>,
}

pub struct ShitarabaClient {
    http: reqwest::Client,
}

impl Default for ShitarabaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ShitarabaClient {
    pub fn new() -> Self {
        Self {
            http: cookie_client(),
        }
    }

    fn parse_loc(url: &str) -> AppResult<BoardLocation> {
        let u = parse_shitaraba(url)
            .ok_or_else(|| AppError::InvalidUrl(format!("not a shitaraba URL: {url}")))?;
        Ok(BoardLocation {
            category: u.category,
            board_id: u.board_id,
            key: u.key,
        })
    }

    fn subject_url(loc: &BoardLocation) -> String {
        format!(
            "https://jbbs.shitaraba.net/{}/{}/subject.txt",
            loc.category, loc.board_id
        )
    }

    fn rawmode_url(loc: &BoardLocation, key: &str) -> String {
        format!(
            "https://jbbs.shitaraba.net/bbs/rawmode.cgi/{}/{}/{}/",
            loc.category, loc.board_id, key
        )
    }

    fn write_url(loc: &BoardLocation) -> String {
        format!(
            "https://jbbs.shitaraba.net/bbs/write.cgi/{}/{}/",
            loc.category, loc.board_id
        )
    }

    /// Fetch and parse `subject.txt`. Decodes EUC-JP.
    pub async fn list_threads(
        &self,
        board_url: &str,
    ) -> AppResult<Vec<super::parse::SubjectEntry>> {
        let loc = Self::parse_loc(board_url)?;
        let url = Self::subject_url(&loc);
        let bytes = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let body = BoardEncoding::EucJp.decode(&bytes)?;
        Ok(parse_shitaraba_subject(&body))
    }

    /// Fetch `rawmode.cgi` and parse it into `Post`s.
    /// `prev` enables incremental fetching by starting after the last
    /// post number we already saw.
    pub async fn fetch_thread(
        &self,
        thread_url: &str,
        prev: Option<&FetchState>,
    ) -> AppResult<(Vec<Post>, FetchState)> {
        let loc = Self::parse_loc(thread_url)?;
        let key = loc
            .key
            .as_deref()
            .ok_or_else(|| AppError::InvalidUrl(format!("missing thread key in: {thread_url}")))?;

        let last_n = prev.map(|s| s.last_count).unwrap_or(0);
        // rawmode supports `/{from}-` to fetch posts >= from.
        // We want strictly greater than last_n, so we ask for last_n+1.
        let start = last_n.saturating_add(1);
        let mut url = Self::rawmode_url(&loc, key);
        if start > 1 {
            url.push_str(&format!("{start}-"));
        }

        let mut req = self.http.get(&url);
        if let Some(p) = prev {
            if let Some(lm) = &p.last_modified {
                req = req.header("If-Modified-Since", lm);
            }
        }
        let resp = req.send().await?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok((Vec::new(), prev.cloned().unwrap_or_default()));
        }
        if !resp.status().is_success() {
            return Err(AppError::Network(format!(
                "rawmode returned {}",
                resp.status()
            )));
        }
        let lm = resp
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().await?;
        let body = BoardEncoding::EucJp.decode(&bytes)?;
        let posts = parse_shitaraba_dat(&body);

        let last_count = posts.last().map(|p| p.number).unwrap_or(last_n);
        let state = FetchState {
            last_modified: lm,
            last_byte: 0,
            last_count,
        };
        Ok((posts, state))
    }

    /// Post to `write.cgi`. The caller is responsible for confirming
    /// with the user before invoking this.
    pub async fn post(&self, thread_url: &str, req: &PostRequest) -> AppResult<()> {
        let loc = Self::parse_loc(thread_url)?;
        let key = loc
            .key
            .as_deref()
            .ok_or_else(|| AppError::InvalidUrl(format!("missing thread key in: {thread_url}")))?;

        let url = Self::write_url(&loc);
        let referer = format!(
            "https://jbbs.shitaraba.net/{}/{}/",
            loc.category, loc.board_id
        );
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // EUC-JP percent-encoding for each field.
        let enc = |s: &str| percent_encode(&BoardEncoding::EucJp.encode(s));
        let body = format!(
            "DIR={}&BBS={}&KEY={}&NAME={}&MAIL={}&MESSAGE={}&TIME={}&submit={}",
            enc(&loc.category),
            enc(&loc.board_id),
            enc(key),
            enc(&req.name),
            enc(&req.mail),
            enc(&req.body),
            time,
            enc("書き込む"),
        );

        let resp = self
            .http
            .post(&url)
            .header("Referer", referer)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        let text_bytes = resp.bytes().await?;
        let text = BoardEncoding::EucJp
            .decode(&text_bytes)
            .unwrap_or_else(|_| String::new());
        // RESULT::CHECK は (旧仕様の) Cookie 確認ステップに使われる
        // ことがあるため成功扱い。残りは post_result の判定に任せる。
        if text.contains("RESULT::CHECK") {
            return Ok(());
        }
        match classify_shitaraba(&text) {
            PostOutcome::Success => Ok(()),
            PostOutcome::NeedsCookieConfirm => Err(AppError::PostRejected(
                "Cookie 確認画面が返ってきました (ブラウザで一度書き込んで確認画面を抜けてください)"
                    .into(),
            )),
            PostOutcome::Rejected(kind) => Err(kind.into_error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_builders() {
        let loc = BoardLocation {
            category: "computer".into(),
            board_id: "4567".into(),
            key: None,
        };
        assert_eq!(
            ShitarabaClient::subject_url(&loc),
            "https://jbbs.shitaraba.net/computer/4567/subject.txt"
        );
        assert_eq!(
            ShitarabaClient::rawmode_url(&loc, "1234"),
            "https://jbbs.shitaraba.net/bbs/rawmode.cgi/computer/4567/1234/"
        );
        assert_eq!(
            ShitarabaClient::write_url(&loc),
            "https://jbbs.shitaraba.net/bbs/write.cgi/computer/4567/"
        );
    }

    #[test]
    fn parse_loc_rejects_non_shitaraba() {
        assert!(ShitarabaClient::parse_loc("https://example.invalid/").is_err());
    }

    #[test]
    fn parse_loc_extracts_key() {
        let loc =
            ShitarabaClient::parse_loc("https://jbbs.shitaraba.net/bbs/read.cgi/c/1/2/").unwrap();
        assert_eq!(loc.category, "c");
        assert_eq!(loc.board_id, "1");
        assert_eq!(loc.key.as_deref(), Some("2"));
    }
}
