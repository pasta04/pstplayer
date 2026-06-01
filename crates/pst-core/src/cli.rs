//! Command-line argument parsing.
//!
//! See `docs/features.md` §2.1.
//!
//! For MVP we hand-roll the parser (clap pulls in 70+ KB to the binary
//! and we only have a couple of dozen flags). Switch to clap if the
//! flag set grows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliArgs {
    /// Positional 1: the PeerCast playlist or stream URL.
    pub url: Option<String>,
    /// Positional 2: channel name (used for the initial title bar).
    pub channel_name: Option<String>,

    pub id: Option<String>,
    pub tip: Option<String>,
    pub contact: Option<String>,
    pub genre: Option<String>,
    pub desc: Option<String>,
    pub bitrate: Option<u32>,
    pub content_type: Option<String>,
    pub comment: Option<String>,
    pub yp_name: Option<String>,
    pub yp_url: Option<String>,

    pub no_autoplay: bool,
    pub no_bbs: bool,
    pub minimized: bool,
    pub show_help: bool,
    pub show_version: bool,
    /// 起動時に強制的に録画も開始する (favorites の auto_record と独立)。
    /// hub から「視聴 + 録画」で spawn された viewer が使う。
    pub record_on_start: bool,
}

/// Parse a slice of arguments (excluding `argv[0]`).
pub fn parse(args: &[String]) -> CliArgs {
    let mut out = CliArgs::default();
    let mut positional = Vec::new();

    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if let Some(rest) = a.strip_prefix("--") {
            if let Some(eq) = rest.find('=') {
                let (k, v) = rest.split_at(eq);
                let v = &v[1..];
                assign_long(&mut out, k, Some(v.to_string()));
            } else {
                match rest {
                    "no-autoplay" => out.no_autoplay = true,
                    "no-bbs" => out.no_bbs = true,
                    "minimized" => out.minimized = true,
                    "help" => out.show_help = true,
                    "version" => out.show_version = true,
                    "record-on-start" => out.record_on_start = true,
                    _ => {
                        let next = iter.next().cloned();
                        assign_long(&mut out, rest, next);
                    }
                }
            }
        } else if let Some(rest) = a.strip_prefix('-') {
            match rest {
                "h" => out.show_help = true,
                "V" => out.show_version = true,
                _ => positional.push(a.clone()),
            }
        } else {
            positional.push(a.clone());
        }
    }

    if let Some(url) = positional.first() {
        out.url = Some(url.clone());
    }
    if let Some(name) = positional.get(1) {
        out.channel_name = Some(name.clone());
    }

    out
}

fn assign_long(out: &mut CliArgs, key: &str, val: Option<String>) {
    match key {
        "name" => out.channel_name = val,
        "id" => out.id = val,
        "tip" => out.tip = val,
        "contact" => out.contact = val,
        "genre" => out.genre = val,
        "desc" => out.desc = val,
        "bitrate" => out.bitrate = val.and_then(|v| v.parse().ok()),
        "type" => out.content_type = val,
        "comment" => out.comment = val,
        "yp-name" => out.yp_name = val,
        "yp-url" => out.yp_url = val,
        _ => { /* unknown long option: ignore for forward compat */ }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn url_only() {
        let a = parse(&argv(&["http://example.invalid:7144/pls/abc"]));
        assert_eq!(
            a.url.as_deref(),
            Some("http://example.invalid:7144/pls/abc")
        );
        assert_eq!(a.channel_name, None);
    }

    #[test]
    fn url_and_name_pcrplayer_style() {
        let a = parse(&argv(&[
            "http://example.invalid:7144/pls/abc",
            "テスト配信",
        ]));
        assert_eq!(a.channel_name.as_deref(), Some("テスト配信"));
    }

    #[test]
    fn long_options() {
        let a = parse(&argv(&[
            "http://h/",
            "--contact=http://bbs/",
            "--genre",
            "Music",
            "--bitrate=320",
            "--no-bbs",
        ]));
        assert_eq!(a.contact.as_deref(), Some("http://bbs/"));
        assert_eq!(a.genre.as_deref(), Some("Music"));
        assert_eq!(a.bitrate, Some(320));
        assert!(a.no_bbs);
    }

    #[test]
    fn help_short() {
        let a = parse(&argv(&["-h"]));
        assert!(a.show_help);
    }

    #[test]
    fn record_on_start_flag() {
        let a = parse(&argv(&["http://h/pls/x", "--record-on-start"]));
        assert!(a.record_on_start);
        assert_eq!(a.url.as_deref(), Some("http://h/pls/x"));
    }

    #[test]
    fn record_on_start_default_off() {
        let a = parse(&argv(&["http://h/pls/x"]));
        assert!(!a.record_on_start);
    }
}
