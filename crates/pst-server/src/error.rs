//! axum ハンドラ用のエラー型。pst-core::AppError をラップしつつ、HTTP
//! ステータスとして表現する。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use pst_core::util::errors::AppError;
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

#[derive(Serialize)]
struct Body<'a> {
    code: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Body {
            code: self.code,
            message: &self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        let (status, code) = match &e {
            AppError::InvalidUrl(_) => (StatusCode::BAD_REQUEST, "invalid_url"),
            AppError::PeerCastUnreachable(_) => (StatusCode::BAD_GATEWAY, "peercast_unreachable"),
            AppError::ThreadGone(_) => (StatusCode::NOT_FOUND, "thread_gone"),
            AppError::BoardRegulated(_) => (StatusCode::FORBIDDEN, "board_regulated"),
            AppError::PostRejected(_) => (StatusCode::UNPROCESSABLE_ENTITY, "post_rejected"),
            AppError::Network(_) => (StatusCode::BAD_GATEWAY, "network"),
            AppError::Decode(_) => (StatusCode::INTERNAL_SERVER_ERROR, "decode"),
            AppError::NotImplemented(_) => (StatusCode::NOT_IMPLEMENTED, "not_implemented"),
        };
        ApiError {
            status,
            code,
            message: e.to_string(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
