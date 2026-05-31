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
        id: fields[1].to_string(),
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
    let resp = CLIENT.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!("YP fetch returned {}", resp.status())));
    }
    let body = resp.text().await?;
    Ok(parse(&body))
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
        assert_eq!(e.listeners, 10);
        assert_eq!(e.relays, 3);
        assert_eq!(e.bitrate, 320);
        assert_eq!(e.desc, "<detail>");
    }

    #[test]
    fn skips_short_lines() {
        assert!(parse_line("too<>few<>fields").is_none());
    }
}
