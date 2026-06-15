//! YP `index.txt` parser (19 fields, UTF-8 on modern YPs).
//!
//! See `docs/protocols/peercast.md` §4.

use serde::{Deserialize, Serialize};

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YpEntry {
    pub name: String,
    pub id: String,
    pub tip: String,
    pub contact_url: String,
    pub genre: String,
    pub desc: String,
    pub listeners: i32,
    pub relays: i32,
    pub bitrate: u32,
    pub content_type: String,
    pub track_artist: String,
    pub track_album: String,
    pub track_title: String,
    pub track_contact: String,
    pub name_url_encoded: String,
    pub uptime: String,
    pub flag_click: String,
    pub comment: String,
    pub flag_extra: String,
    /// 取得元 YP の名前 (複数 YP マージ後に `fetch_indexes` がセット)。
    /// 単独 `fetch_index` 経由では空文字列のまま。
    #[serde(default)]
    pub yp_source: String,
}

fn unescape(field: &str) -> String {
    field.replace("&lt;", "<").replace("&gt;", ">")
}

fn parse_int(s: &str) -> i32 {
    s.parse::<i32>().unwrap_or(0)
}

fn parse_uint(s: &str) -> u32 {
    s.parse::<u32>().unwrap_or(0)
}

/// Parse one `<>`-separated line of an `index.txt` file.
/// Returns `None` if the line does not have at least 19 fields.
pub fn parse_line(line: &str) -> Option<YpEntry> {
    let fields: Vec<&str> = line.split("<>").collect();
    if fields.len() < 19 {
        return None;
    }
    Some(YpEntry {
        name: unescape(fields[0]),
        // channel_id は小文字へ正規化する。視聴ウィンドウの single_instance
        // ロックは ChannelId::parse 経由で小文字化されるため、ハブの「視聴中 /
        // 録画中」判定 (watchingIds.has(e.id)) が大文字のままだと一致しない。
        id: fields[1].to_ascii_lowercase(),
        tip: fields[2].to_string(),
        contact_url: fields[3].to_string(),
        genre: unescape(fields[4]),
        desc: unescape(fields[5]),
        listeners: parse_int(fields[6]),
        relays: parse_int(fields[7]),
        bitrate: parse_uint(fields[8]),
        content_type: fields[9].to_string(),
        track_artist: unescape(fields[10]),
        track_album: unescape(fields[11]),
        track_title: unescape(fields[12]),
        track_contact: fields[13].to_string(),
        name_url_encoded: fields[14].to_string(),
        uptime: fields[15].to_string(),
        flag_click: fields[16].to_string(),
        comment: unescape(fields[17]),
        flag_extra: fields[18].to_string(),
        yp_source: String::new(),
    })
}

/// Parse an entire `index.txt` body, skipping malformed lines.
pub fn parse(body: &str) -> Vec<YpEntry> {
    body.lines().filter_map(parse_line).collect()
}

/// Fetch a YP `index.txt` from `yp_url` and return parsed entries.
/// `yp_url` should be the absolute URL of the index file
/// (例: `http://yp.example.invalid/index.txt`)。空文字列は早期エラー。
pub async fn fetch_index(yp_url: &str) -> AppResult<Vec<YpEntry>> {
    let url = yp_url.trim();
    if url.is_empty() {
        return Err(AppError::InvalidUrl("YP URL is empty".into()));
    }
    // SSRF 対策: file:// や gopher:// 等で内部リソースを舐められないよう
    // http(s) のみ許可。reqwest 単体は scheme 制限がないので呼び出し側で
    // 弾く必要がある。
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::InvalidUrl(format!(
            "YP URL must start with http:// or https://: {url}"
        )));
    }
    let resp = CLIENT.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!(
            "YP fetch returned {}",
            resp.status()
        )));
    }
    let body = resp.text().await?;
    Ok(parse(&body))
}

/// 複数の YP ソースを並行 fetch して結果をマージする。同一 `channel_id`
/// が複数 YP に出てきた場合は **最初の YP (= 引数の配列の先頭側)** を
/// 採用 (= ソース並びが優先度)。`yp_source` フィールドに採用元 YP の
/// 名前 (`YpSource::name`) が入る。
///
/// 個別 YP の fetch 失敗は他に影響させない (除外して継続) — 失敗一覧は
/// `MultiFetchOutcome::failures` に積む。
pub async fn fetch_indexes(sources: &[crate::config::schema::YpSource]) -> MultiFetchOutcome {
    // 並行 fetch (tokio::task::JoinSet) + 元の並び順を保つために index 付き
    // で集計、最後に並び替える。各 YP の失敗は他に影響させない。
    let mut set = tokio::task::JoinSet::new();
    for (idx, s) in sources.iter().enumerate() {
        let src = s.clone();
        set.spawn(async move {
            let res = fetch_index(&src.url).await;
            (idx, src, res)
        });
    }

    let mut indexed: Vec<(
        usize,
        crate::config::schema::YpSource,
        AppResult<Vec<YpEntry>>,
    )> = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(triple) = joined {
            indexed.push(triple);
        }
    }
    indexed.sort_by_key(|t| t.0);

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut entries: Vec<YpEntry> = Vec::new();
    let mut failures: Vec<YpFetchFailure> = Vec::new();

    for (_, src, res) in indexed {
        match res {
            Ok(list) => {
                for mut e in list {
                    let key = e.id.to_ascii_lowercase();
                    if key.is_empty() || seen.contains(&key) {
                        continue;
                    }
                    seen.insert(key);
                    e.yp_source = src.name.clone();
                    entries.push(e);
                }
            }
            Err(err) => failures.push(YpFetchFailure {
                source: src.name.clone(),
                url: src.url.clone(),
                error: err.to_string(),
            }),
        }
    }

    MultiFetchOutcome { entries, failures }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MultiFetchOutcome {
    pub entries: Vec<YpEntry>,
    pub failures: Vec<YpFetchFailure>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct YpFetchFailure {
    pub source: String,
    pub url: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_entry() {
        let line = [
            "TestCh",
            "0123456789ABCDEF0123456789ABCDEF",
            "192.0.2.1:7144",
            "http://example.invalid/bbs",
            "Music",
            "&lt;detail&gt;",
            "10",
            "3",
            "320",
            "FLV",
            "Artist",
            "Album",
            "Title",
            "http://example.invalid/track",
            "TestCh",
            "1:23",
            "click",
            "hello",
            "0",
        ]
        .join("<>");
        let e = parse_line(&line).unwrap();
        assert_eq!(e.name, "TestCh");
        // channel_id は小文字化される (視聴中/録画中カウントの ID 照合のため)。
        assert_eq!(e.id, "0123456789abcdef0123456789abcdef");
        assert_eq!(e.listeners, 10);
        assert_eq!(e.relays, 3);
        assert_eq!(e.bitrate, 320);
        assert_eq!(e.desc, "<detail>");
    }

    #[test]
    fn skips_short_lines() {
        assert!(parse_line("too<>few<>fields").is_none());
    }

    #[tokio::test]
    async fn fetch_index_rejects_non_http_schemes() {
        for url in [
            "file:///etc/passwd",
            "gopher://internal/x",
            "ftp://yp.example/index.txt",
            "javascript:alert(1)",
        ] {
            let err = fetch_index(url).await.expect_err(url);
            assert!(matches!(err, AppError::InvalidUrl(_)), "{url}: {err:?}");
        }
    }

    #[tokio::test]
    async fn fetch_index_accepts_http_and_https_prefix() {
        // 実際にネットワークに繋がる URL は使わない。scheme チェックを
        // 通過すると、次の接続フェーズで Network error を返すはず。
        let err = fetch_index("http://127.0.0.1:0/index.txt")
            .await
            .expect_err("expected connect error");
        assert!(!matches!(err, AppError::InvalidUrl(_)), "{err:?}");
    }
}
