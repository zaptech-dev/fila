use std::sync::Arc;

use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use rapina::extract::{FromRequest, PathParams};
use rapina::http::Request;
use rapina::prelude::*;
use rapina::state::AppState;
use serde::de::DeserializeOwned;
use sha2::Sha256;

use crate::config::app::AppConfig;

type HmacSha256 = Hmac<Sha256>;

/// A body extractor that verifies the GitHub webhook HMAC-SHA256 signature
/// before deserializing the payload.
pub struct SignedBody<T>(pub T);

impl<T> SignedBody<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for SignedBody<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: DeserializeOwned + Send> FromRequest for SignedBody<T> {
    async fn from_request(
        req: Request<hyper::body::Incoming>,
        _params: &PathParams,
        state: &Arc<AppState>,
    ) -> std::result::Result<Self, rapina::error::Error> {
        let config = state
            .get::<AppConfig>()
            .ok_or_else(|| Error::internal("AppConfig not registered in state"))?;
        let secret = &config.github_webhook_secret;

        let signature: Option<String> = req
            .headers()
            .get("X-Hub-Signature-256")
            .and_then(|v: &rapina::http::HeaderValue| v.to_str().ok())
            .map(|s: &str| s.to_string());

        let body = req.into_body();
        let bytes: bytes::Bytes = body
            .collect()
            .await
            .map_err(|_| Error::bad_request("Failed to read request body"))?
            .to_bytes();

        let sig_header =
            signature.ok_or_else(|| Error::unauthorized("Missing X-Hub-Signature-256 header"))?;

        let hex_sig = sig_header
            .strip_prefix("sha256=")
            .ok_or_else(|| Error::unauthorized("Invalid signature format"))?;

        let expected = hex::decode(hex_sig)
            .map_err(|_| Error::unauthorized("Invalid signature hex encoding"))?;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|_| Error::internal("Invalid webhook secret key"))?;
        mac.update(&bytes);

        mac.verify_slice(&expected)
            .map_err(|_| Error::unauthorized("Webhook signature verification failed"))?;

        let value: T = serde_json::from_slice(&bytes)
            .map_err(|e| Error::bad_request(format!("Invalid JSON in request body: {e}")))?;

        Ok(SignedBody(value))
    }
}
