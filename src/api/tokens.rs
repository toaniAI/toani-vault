use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TokenStatsResponse {
    pub active_tokens: u64,
}

pub fn token_routes() -> Router {
    Router::new().route("/tokens/stats", get(get_token_stats_handler))
}

async fn get_token_stats_handler() -> Json<TokenStatsResponse> {
    Json(TokenStatsResponse { active_tokens: 0 })
}
