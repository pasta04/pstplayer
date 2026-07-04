//! BBS 投稿レスポンスの分類。2ch / したらば の双方で「規制」「Cookie 確認」
//! 「ヒラ拒否」を共通の AppError バリアントにマップする。
//!
//! 規制パターンは BBS 側が変わるたびに調整が必要なので、ここに集約して
//! テストしやすくしている。

use crate::util::errors::AppError;

/// 投稿レスポンス本文 (デコード済みの HTML テキスト) を判別する。
/// 成功 / Cookie 確認 / 規制 / その他拒否 のどれかを返す。
#[derive(Debug, PartialEq, Eq)]
pub enum PostOutcome {
    /// 投稿が完了した。
    Success,
    /// 2 段階確認が必要 (cookie / check)。同じ POST を Cookie 付きで
    /// 再送すれば通る想定。
    NeedsCookieConfirm,
    /// 規制 / 拒否系。
    Rejected(RejectKind),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RejectKind {
    /// ホスト規制 / 連投規制 / プロバ規制 / 巻き込まれ規制 等。
    Regulated(String),
    /// 規制以外の拒否 (フォーム改変 / 不正な input / 確認失敗等)。
    Other(String),
}

impl RejectKind {
    pub fn into_error(self) -> AppError {
        match self {
            RejectKind::Regulated(s) => AppError::BoardRegulated(s),
            RejectKind::Other(s) => AppError::PostRejected(s),
        }
    }
}

/// 2ch 系応答 (HTML in Shift_JIS) を判定。
pub fn classify_ch2(body: &str) -> PostOutcome {
    if body.contains("<!-- 2ch_X:true -->") {
        return PostOutcome::Success;
    }
    if body.contains("<!-- 2ch_X:cookie -->") || body.contains("<!-- 2ch_X:check -->") {
        return PostOutcome::NeedsCookieConfirm;
    }
    // 2ch_X コメントを出さない互換実装 (0ch 系等) は、書き込み成功時に
    // 従来の完了画面テキストだけを返す (komokomo.ddns.net で実測: 成功
    // なのに拒否扱いになっていた)。確認画面にはこの文言は出ない。
    if body.contains("書きこみました") || body.contains("書き込みました") {
        return PostOutcome::Success;
    }
    PostOutcome::Rejected(classify_reject(body))
}

/// したらば応答 (HTML in EUC-JP) を判定。
/// したらばは 2ch_X コメントを出さないので、本文文字列で判定する。
pub fn classify_shitaraba(body: &str) -> PostOutcome {
    // 完了画面で必ず出る代表的な文字列。
    if body.contains("書きこみました") || body.contains("書き込みました") {
        return PostOutcome::Success;
    }
    // クッキー確認系。
    // - `RESULT::CHECK` は「Cookie 確認画面に遷移しろ」のシグナル。
    //   ブラウザは即座に同じ POST を再送する設計で、本クライアントも
    //   Cookie Jar が更新済みなので 2 度目の POST で通る (= ch2 系と
    //   同じ二段階確認パターン)。決して成功扱いにしない (旧実装は
    //   ここで Ok を返してしまっていたため、書き込みが反映されていない
    //   のに UI 上は「投稿成功」と出るバグがあった)。
    // - 「クッキー … 有効」を含む確認画面 (旧仕様)。
    if body.contains("RESULT::CHECK") || (body.contains("クッキー") && body.contains("有効"))
    {
        return PostOutcome::NeedsCookieConfirm;
    }
    PostOutcome::Rejected(classify_reject(body))
}

/// 規制系か単なる拒否かを文字列パターンから判別する。両 BBS 共通。
fn classify_reject(body: &str) -> RejectKind {
    // よく見られる規制キーワード (= ホスト / プロバイダ単位の拒否)。
    const REG_KEYS: &[&str] = &[
        "規制中",
        "ホスト規制",
        "プロバ規制",
        "巻き込まれ",
        "アクセス規制",
        "!=BANNED!=",
        "!=Banned!=",
        "Banned!",
        "ERROR: 規制",
        "書き込めません",
        "投稿できません",
    ];
    for kw in REG_KEYS {
        if body.contains(kw) {
            return RejectKind::Regulated(snippet(body));
        }
    }
    RejectKind::Other(snippet(body))
}

/// レスポンス HTML の見出しっぽい部分を 200 文字まで抜く。ユーザー向けの
/// メッセージに添えるためのスニペット。
fn snippet(body: &str) -> String {
    // タグを単純に剥がしてから先頭 200 文字。空白の連続も縮める。
    let mut text = String::new();
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ch2_success_plain_text_without_marker() {
        // 2ch_X マーカーを出さない互換実装の成功画面。
        let body = "<html><head><title>書きこみました。</title></head>\
             <body>書きこみが終わりました。<br>画面を切り替えるまでしばらくお待ち下さい。</body></html>";
        assert_eq!(classify_ch2(body), PostOutcome::Success);
    }

    #[test]
    fn ch2_success_marker() {
        assert_eq!(
            classify_ch2("<html><!-- 2ch_X:true -->done</html>"),
            PostOutcome::Success
        );
    }

    #[test]
    fn ch2_cookie_check() {
        assert_eq!(
            classify_ch2("<!-- 2ch_X:cookie -->cookie please"),
            PostOutcome::NeedsCookieConfirm
        );
        assert_eq!(
            classify_ch2("<!-- 2ch_X:check -->check please"),
            PostOutcome::NeedsCookieConfirm
        );
    }

    #[test]
    fn ch2_regulated() {
        let html = "<html><body><h1>ERROR</h1>ホスト規制中です。</body></html>";
        match classify_ch2(html) {
            PostOutcome::Rejected(RejectKind::Regulated(_)) => {}
            other => panic!("expected Regulated, got {other:?}"),
        }
    }

    #[test]
    fn ch2_other_reject() {
        let html = "<html><body>不正なフォームです</body></html>";
        match classify_ch2(html) {
            PostOutcome::Rejected(RejectKind::Other(_)) => {}
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn shitaraba_success_japanese() {
        assert_eq!(
            classify_shitaraba("<html>書きこみました</html>"),
            PostOutcome::Success
        );
        assert_eq!(
            classify_shitaraba("<html>書き込みました</html>"),
            PostOutcome::Success
        );
    }

    #[test]
    fn shitaraba_regulated() {
        let body = "アクセス規制対象です。";
        match classify_shitaraba(body) {
            PostOutcome::Rejected(RejectKind::Regulated(_)) => {}
            other => panic!("expected Regulated, got {other:?}"),
        }
    }

    #[test]
    fn shitaraba_result_check_is_cookie_confirm_not_success() {
        // RESULT::CHECK は「Cookie 確認画面に遷移しろ」のサーバシグナル。
        // ブラウザは即座に同じ POST を再送する。本クライアントも二段階
        // 確認として扱う必要があり、これを Success にすると書き込み
        // していないのに UI 上「投稿成功」と表示されるバグになる。
        assert_eq!(
            classify_shitaraba("RESULT::CHECK"),
            PostOutcome::NeedsCookieConfirm
        );
        assert_eq!(
            classify_shitaraba("<html>some preamble RESULT::CHECK trailer</html>"),
            PostOutcome::NeedsCookieConfirm
        );
    }

    #[test]
    fn shitaraba_legacy_cookie_confirm() {
        assert_eq!(
            classify_shitaraba("<html>クッキーを有効にしてください</html>"),
            PostOutcome::NeedsCookieConfirm
        );
    }

    #[test]
    fn snippet_strips_tags() {
        let s = snippet("<html><body>  Hello\n\nworld!  </body></html>");
        assert_eq!(s, "Hello world!");
    }
}
