//! Anchor (`>>N`) extraction.
//!
//! See `docs/protocols/bbs.md` §5.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorRange {
    pub from: u32,
    pub to: u32,
}

static ANCHOR_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches >>1 and >>1-5 (Japanese full-width >> also tolerated).
    Regex::new(r"(?:>>|≫|＞＞)(\d+)(?:-(\d+))?").expect("anchor regex compiles")
});

/// Find all `>>N` or `>>N-M` anchors inside a post body.
pub fn find_anchors(text: &str) -> Vec<AnchorRange> {
    ANCHOR_RE
        .captures_iter(text)
        .filter_map(|cap| {
            let from = cap.get(1)?.as_str().parse::<u32>().ok()?;
            let to = cap.get(2).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(from);
            Some(AnchorRange { from, to })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_anchor() {
        assert_eq!(find_anchors(">>42 hi"), vec![AnchorRange { from: 42, to: 42 }]);
    }

    #[test]
    fn range_anchor() {
        assert_eq!(find_anchors("see >>10-15"), vec![AnchorRange { from: 10, to: 15 }]);
    }

    #[test]
    fn multiple_anchors() {
        let v = find_anchors(">>1 >>3-4");
        assert_eq!(v, vec![AnchorRange { from: 1, to: 1 }, AnchorRange { from: 3, to: 4 },]);
    }

    #[test]
    fn no_anchors() {
        assert!(find_anchors("just text").is_empty());
    }

    #[test]
    fn fullwidth_arrows() {
        assert_eq!(find_anchors("＞＞7"), vec![AnchorRange { from: 7, to: 7 }]);
    }
}
