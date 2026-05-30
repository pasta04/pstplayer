//! Playlist parsing for `.pls` and `.m3u` content returned by
//! `http://host:port/pls/{id}`.
//!
//! See `docs/protocols/peercast.md` §2 for the URL structure that wraps
//! these playlists.

use crate::util::errors::{AppError, AppResult};

/// Pick the first stream URL out of a PLS / M3U body.
///
/// * PLS: looks for `File1=...` lines (key/value, INI-style).
/// * M3U: takes the first non-comment, non-empty line.
///
/// PeerCast normally returns PLS for `/pls/{id}` even when no extension
/// is supplied, but we accept either form to be robust against quirks.
pub fn first_stream_url(body: &str) -> AppResult<String> {
    let trimmed = body.trim_start_matches('\u{feff}');

    // PLS format: contains `[playlist]` header.
    if trimmed.lines().any(|l| l.trim().eq_ignore_ascii_case("[playlist]")) {
        for line in trimmed.lines() {
            let line = line.trim();
            // Match "FileN=" (N is digits), case-insensitive on the key.
            if let Some(eq_pos) = line.find('=') {
                let (key, value) = line.split_at(eq_pos);
                let key_lower = key.to_ascii_lowercase();
                if key_lower.starts_with("file")
                    && key_lower[4..].chars().all(|c| c.is_ascii_digit())
                {
                    let v = value.trim_start_matches('=').trim();
                    if !v.is_empty() {
                        return Ok(v.to_string());
                    }
                }
            }
        }
        return Err(AppError::Decode("PLS playlist has no File= entry".into()));
    }

    // M3U fallback: first non-comment, non-empty line.
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        return Ok(line.to_string());
    }

    Err(AppError::Decode("playlist body is empty".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pls_simple() {
        let body = "\
[playlist]
NumberOfEntries=1
File1=http://example.invalid:7144/stream/abc.flv
Length1=-1
Version=2
";
        assert_eq!(first_stream_url(body).unwrap(), "http://example.invalid:7144/stream/abc.flv");
    }

    #[test]
    fn pls_with_bom() {
        let body = "\u{feff}[playlist]\nFile1=http://example.invalid/stream\n";
        assert_eq!(first_stream_url(body).unwrap(), "http://example.invalid/stream");
    }

    #[test]
    fn pls_higher_index_only() {
        // Some servers emit File2 etc. before File1; we should still find one.
        let body = "[playlist]\nFile2=http://b/\nFile1=http://a/\n";
        assert_eq!(first_stream_url(body).unwrap(), "http://b/");
    }

    #[test]
    fn m3u_basic() {
        let body = "#EXTM3U\n#EXTINF:-1,Test\nhttp://example.invalid/stream\n";
        assert_eq!(first_stream_url(body).unwrap(), "http://example.invalid/stream");
    }

    #[test]
    fn empty_body_errors() {
        assert!(first_stream_url("   \n   \n").is_err());
    }

    #[test]
    fn pls_with_no_file_errors() {
        assert!(first_stream_url("[playlist]\nNumberOfEntries=0\n").is_err());
    }
}
