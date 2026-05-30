use super::types::{FetchState, Post, PostRequest, ThreadSummary};
use crate::util::errors::AppResult;

// We rely on the stable `async fn` in trait (Rust 1.75+). When we need
// `Box<dyn BoardClient>` later we'll either keep the warning explicit
// or switch to `#[async_trait::async_trait]`.
#[allow(async_fn_in_trait)]
pub trait BoardClient: Send + Sync {
    async fn list_threads(&self, board_url: &str) -> AppResult<Vec<ThreadSummary>>;

    async fn fetch_thread(
        &self,
        thread_url: &str,
        prev: Option<&FetchState>,
    ) -> AppResult<(Vec<Post>, FetchState)>;

    async fn post(&self, thread_url: &str, req: &PostRequest) -> AppResult<()>;
}
