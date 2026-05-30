use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardKind {
    Shitaraba,
    Ch2Compat,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub key: String,
    pub title: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Post {
    pub number: u32,
    pub name: String,
    pub mail: String,
    pub date: String,
    pub id: String,
    pub body: String,
    pub thread_title: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FetchState {
    pub last_modified: Option<String>,
    pub last_byte: u64,
    pub last_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostRequest {
    pub name: String,
    pub mail: String,
    pub body: String,
}
