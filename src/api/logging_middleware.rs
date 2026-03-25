//! 请求/响应日志拦截器中间件
//!
//! 作为最外层 Tower 中间件，为每个 HTTP 请求生成唯一 request_id，
//! 记录请求入口和响应出口日志，按状态码分级输出。

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;

/// 请求/响应日志拦截器
///
/// 记录：入站请求（method, uri, request_id）和出站响应（status, latency_ms）。
/// 5xx -> ERROR, 4xx -> WARN, 其余 -> INFO。
pub async fn request_logging_middleware(req: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let version = format!("{:?}", req.version());

    info!(
        request_id = %request_id,
        method = %method,
        uri = %uri,
        version = %version,
        "incoming request"
    );

    let start = Instant::now();
    let response = next.run(req).await;
    let latency = start.elapsed();
    let status = response.status().as_u16();

    if status >= 500 {
        error!(
            request_id = %request_id,
            method = %method,
            uri = %uri,
            status = status,
            latency_ms = latency.as_millis() as u64,
            "server error response"
        );
    } else if status >= 400 {
        warn!(
            request_id = %request_id,
            method = %method,
            uri = %uri,
            status = status,
            latency_ms = latency.as_millis() as u64,
            "client error response"
        );
    } else {
        info!(
            request_id = %request_id,
            method = %method,
            uri = %uri,
            status = status,
            latency_ms = latency.as_millis() as u64,
            "response"
        );
    }

    response
}
