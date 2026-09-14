//! Character encoding and HTML entity helpers for the BBS layer.
//!
//! See `docs/protocols/bbs.md` §1 and §6.

use crate::util::errors::AppResult;
use encoding_rs::{EUC_JP, SHIFT_JIS, UTF_8};

#[derive(Debug, Clone, Copy)]
pub enum BoardEncoding {
    ShiftJis,
    EucJp,
    Utf8,
}

impl BoardEncoding {
    fn to_encoding(self) -> &'static encoding_rs::Encoding {
        match self {
            BoardEncoding::ShiftJis => SHIFT_JIS,
            BoardEncoding::EucJp => EUC_JP,
            BoardEncoding::Utf8 => UTF_8,
        }
    }

    pub fn decode(self, bytes: &[u8]) -> AppResult<String> {
        // encoding_rs は不正バイトを U+FFFD (REPLACEMENT CHARACTER) で
        // 置換しつつ `had_errors=true` を返す。BBS では稀に外字 / 機種依存
        // 文字 / 絵文字 / 別エンコーディングの混入が起きるが、それを
        // 全体 Err にすると 1 バイトの不正でスレッドが完全に表示できなく
        // なってしまう (実害が大きい)。よって had_errors でも置換後の
        // 文字列をそのまま返し、debug ログだけ出す方針。
        let (cow, _, had_errors) = self.to_encoding().decode(bytes);
        if had_errors {
            eprintln!(
                "bbs decode: {:?} で {} バイト中に置換が発生 (U+FFFD で継続)",
                self,
                bytes.len()
            );
        }
        Ok(cow.into_owned())
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        let (cow, _, _) = self.to_encoding().encode(text);
        cow.into_owned()
    }
}

/// Percent-encode every byte that is not an unreserved character per RFC 3986.
/// Used when posting form bodies whose values are already encoded in
/// the board's native charset (Shift_JIS / EUC-JP).
pub fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decode HTML entities found in 2ch/shitaraba dat bodies.
/// Restricted to the common subset (`&lt;` `&gt;` `&amp;` `&quot;`
/// `&#NNN;` `&#xHHH;`) plus the shitaraba-specific `&#65374;` → `～`.
pub fn unescape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = match after.find(';') {
            Some(p) if p <= 8 => p,
            _ => {
                out.push('&');
                rest = after;
                continue;
            }
        };
        let entity = &after[..semi];
        let replacement = match entity {
            "lt" => Some('<'.to_string()),
            "gt" => Some('>'.to_string()),
            "amp" => Some('&'.to_string()),
            "quot" => Some('"'.to_string()),
            _ if entity.starts_with('#') => {
                let num = &entity[1..];
                let code =
                    if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num.parse::<u32>().ok()
                    };
                code.and_then(char::from_u32).map(|c| c.to_string())
            }
            _ => None,
        };
        match replacement {
            Some(s) => {
                out.push_str(&s);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_shift_jis_roundtrip() {
        let text = "テスト 123";
        let bytes = BoardEncoding::ShiftJis.encode(text);
        let back = BoardEncoding::ShiftJis.decode(&bytes).unwrap();
        assert_eq!(back, text);
    }

    #[test]
    fn encode_decode_euc_jp_roundtrip() {
        let text = "したらば";
        let bytes = BoardEncoding::EucJp.encode(text);
        let back = BoardEncoding::EucJp.decode(&bytes).unwrap();
        assert_eq!(back, text);
    }

    #[test]
    fn html_entities_basic() {
        assert_eq!(unescape_html("a&lt;b&gt;c"), "a<b>c");
        assert_eq!(unescape_html("&amp;&quot;"), "&\"");
    }

    #[test]
    fn numeric_entities() {
        assert_eq!(unescape_html("&#65;&#x42;"), "AB");
    }

    #[test]
    fn shitaraba_tilde() {
        // U+FF5E ＝ &#65374;
        assert_eq!(unescape_html("&#65374;"), "～");
    }

    #[test]
    fn passes_through_bare_ampersand() {
        assert_eq!(unescape_html("at & t"), "at & t");
    }

    #[test]
    fn percent_encode_basic() {
        assert_eq!(percent_encode(b"hello"), "hello");
        assert_eq!(percent_encode(b" "), "%20");
        assert_eq!(percent_encode(b"a=b&c"), "a%3Db%26c");
        // EUC-JP-encoded katakana "ア" (0xA5 0xA2).
        assert_eq!(percent_encode(&[0xA5, 0xA2]), "%A5%A2");
    }

    #[test]
    fn decode_returns_replacement_for_partial_corruption() {
        // Shift_JIS の中に EUC-JP / UTF-8 由来の不正バイトが 1 か所だけ
        // 混ざっても、スレッド全体が読めなくならず U+FFFD で続行できる。
        // ("ア" = 0xA5 0xA2 は EUC-JP のバイト列で、Shift_JIS としては
        // 不完全な先頭バイト 0xA5 になる。0xA2 は 0xA1-0xDF 範囲の半角
        // カナとして解釈されてしまうが、いずれにせよ Err にしない)。
        let mixed = b"hello\xA5\xA2world";
        let out = BoardEncoding::ShiftJis.decode(mixed).unwrap();
        assert!(out.starts_with("hello"));
        assert!(out.ends_with("world"));

        // 完全に不正なバイト列でも Err にせず、置換後の文字列を返す。
        let bad = b"head\xFF\xFE\xFDtail";
        let out = BoardEncoding::EucJp.decode(bad).unwrap();
        assert!(out.contains("head"));
        assert!(out.contains("tail"));
    }
}
