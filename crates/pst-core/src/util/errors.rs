use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// 汎用ネットワークエラー (タイムアウト / TLS / DNS など)。
    /// 接続自体ができない場合は `PeerCastUnreachable` のほうが
    /// ユーザー向けメッセージを出しやすい。
    #[error("network error: {0}")]
    Network(String),

    /// PeerCast 本体に接続できない (起動していない / port が違う /
    /// firewall に塞がれている)。
    #[error("PeerCast に接続できません: {0}")]
    PeerCastUnreachable(String),

    /// スレッドが見つからない (404 / 410)。dat が削除されたか過去
    /// ログ送りになった場合。
    #[error("スレッドが見つかりません: {0}")]
    ThreadGone(String),

    /// BBS への投稿が規制で弾かれた (ホスト規制 / 連投規制 等)。
    #[error("書き込みが規制されました: {0}")]
    BoardRegulated(String),

    /// BBS への投稿が拒否された (規制以外。Cookie 確認失敗、フォーム
    /// 変更等)。
    #[error("書き込みが拒否されました: {0}")]
    PostRejected(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

impl AppError {
    /// BBS 通信のエラー向け補正。`From<reqwest::Error>` は接続失敗を
    /// PeerCast 本体向けの案内 (`PeerCastUnreachable`) に変換するが、
    /// BBS サーバへの接続失敗に同じ案内を出すと誤解を招く (実機 QA:
    /// komokomo の一時的な応答不良が「PeerCast に接続できません。
    /// 設定 → ホスト / ポートを確認」と表示され続けた)。
    #[must_use]
    pub fn for_bbs(self) -> AppError {
        match self {
            AppError::PeerCastUnreachable(m) => {
                AppError::Network(format!("BBS サーバに接続できません: {m}"))
            }
            other => other,
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        // 接続そのものが拒否された / DNS 失敗 / タイムアウト等は
        // ユーザー向けに「相手 (主に PeerCast 本体) が見つからない」
        // メッセージで返す。reqwest::Error の is_connect / is_timeout
        // は status 系のエラーと区別される。
        if e.is_connect() || e.is_timeout() {
            AppError::PeerCastUnreachable(e.to_string())
        } else {
            AppError::Network(e.to_string())
        }
    }
}

impl From<url::ParseError> for AppError {
    fn from(e: url::ParseError) -> Self {
        AppError::InvalidUrl(e.to_string())
    }
}

/// Serializable form returned across the Tauri IPC boundary.
#[derive(Debug, serde::Serialize)]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
}

impl From<AppError> for IpcError {
    fn from(e: AppError) -> Self {
        let code = match e {
            AppError::InvalidUrl(_) => "invalid_url",
            AppError::Network(_) => "network",
            AppError::PeerCastUnreachable(_) => "peercast_unreachable",
            AppError::ThreadGone(_) => "thread_gone",
            AppError::BoardRegulated(_) => "board_regulated",
            AppError::PostRejected(_) => "post_rejected",
            AppError::Decode(_) => "decode",
            AppError::NotImplemented(_) => "not_implemented",
        };
        IpcError {
            code,
            message: e.to_string(),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
