use crate::error::AppError;
use crate::interface::{Request, Response};
use crate::utils::sha1;
use hyper::header::{HeaderMap, HeaderValue, CONNECTION, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::upgrade::Upgraded;
use hyper::Body;
use std::future::Future;
pub use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};
use tokio_tungstenite::WebSocketStream;

pub fn check_websocket_header(headers: &HeaderMap) -> Result<HeaderValue, AppError> {
    let upgrade = headers
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest(String::new()))?;
    if upgrade.trim() != "websocket" {
        return Err(AppError::BadRequest(String::new()));
    }
    let connection = headers
        .get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest(String::new()))?;
    if connection.find("Upgrade").is_none() {
        return Err(AppError::BadRequest(String::new()));
    }
    let mut key = headers
        .get(SEC_WEBSOCKET_KEY)
        .and_then(|key| key.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Failed to read ws key from headers".to_string()))?
        .to_string();
    key.push_str("258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = base64::encode(sha1(key.as_bytes()).as_ref());
    HeaderValue::from_str(&*accept).map_err(error_unexpected!())
}

pub fn establish_web_socket<H, F>(req: Request, handler: H) -> Result<Response, AppError>
where
    H: FnOnce(WebSocketStream<Upgraded>) -> F,
    H: Send + 'static,
    F: Future<Output = ()> + Send,
{
    use hyper::{header, StatusCode};
    use tokio_tungstenite::tungstenite::protocol::Role;
    let accept = check_websocket_header(req.headers())?;
    tokio::spawn(async {
        match req.into_body().on_upgrade().await {
            Ok(upgraded) => {
                let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await;
                log::debug!("WebSocket connection established");
                handler(ws_stream).await;
            }
            Err(e) => {
                log::error!("Failed to upgrade connection: {}", e);
            }
        }
        log::debug!("WebSocket disconnected");
    });
    hyper::Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, accept)
        .body(Body::empty())
        .map_err(error_unexpected!())
}
