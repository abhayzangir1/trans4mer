use reqwest::Client;
use std::time::Duration;
use tracing::{debug, error, info};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::langfuse::LangfuseConfig;

use super::batch::IngestionBatch;

#[derive(Clone)]
pub struct LangfuseClient {
    http_client: Client,
}

impl LangfuseClient {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { http_client }
    }

    /// Dispatches an ingestion batch to the configured Langfuse instance.
    /// NON-NEGOTIABLE INVARIANT: If Langfuse is disabled or unset, ZERO network calls are made.
    pub async fn ingest_batch(
        &self,
        batch: &IngestionBatch,
        config: &LangfuseConfig,
    ) -> Result<(), Trans4mersError> {
        // 1. HARD ZERO-EGRESS GUARD
        if !config.enabled || config.host.trim().is_empty() {
            debug!("Langfuse is disabled or unset; zero network egress executed.");
            return Ok(());
        }

        if batch.is_empty() {
            return Ok(());
        }

        let pub_key = config.public_key.as_deref().unwrap_or("").trim();
        let sec_key = config.secret_key.as_deref().unwrap_or("").trim();

        if pub_key.is_empty() || sec_key.is_empty() {
            debug!("Langfuse public or secret key missing from configuration; skipping egress.");
            return Ok(());
        }

        let url = format!("{}/api/public/ingestion", config.host.trim_end_matches('/'));

        info!(
            "Ingesting {} items to Langfuse endpoint: {}",
            batch.len(),
            url
        );

        let response = self
            .http_client
            .post(&url)
            .basic_auth(pub_key, Some(sec_key))
            .json(batch)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to connect to Langfuse: {}", e);
                Trans4mersError::Network(format!("Failed to connect to Langfuse: {}", e))
            })?;

        let status = response.status();
        if status.is_success() {
            debug!("Langfuse batch ingestion accepted with status: {}", status);
            Ok(())
        } else {
            let err_body = response.text().await.unwrap_or_default();
            error!("Langfuse returned error status {}: {}", status, err_body);
            Err(Trans4mersError::Network(format!(
                "Langfuse ingestion rejected (HTTP {}): {}",
                status, err_body
            )))
        }
    }

    /// Verifies connectivity and credentials against a Langfuse instance.
    /// Initiated ONLY upon explicit user action.
    pub async fn test_connection(
        &self,
        host: &str,
        public_key: Option<&str>,
        secret_key: Option<&str>,
    ) -> Result<String, Trans4mersError> {
        let clean_host = host.trim().trim_end_matches('/');
        if clean_host.is_empty() {
            return Err(Trans4mersError::Validation(
                "Langfuse host endpoint URL cannot be empty".to_string(),
            ));
        }

        let url = format!("{}/api/public/health", clean_host);

        let mut req = self.http_client.get(&url);
        if let (Some(pub_k), Some(sec_k)) = (public_key, secret_key)
            && !pub_k.trim().is_empty()
            && !sec_k.trim().is_empty()
        {
            req = req.basic_auth(pub_k.trim(), Some(sec_k.trim()));
        }

        let res = req.send().await.map_err(|e| {
            Trans4mersError::Network(format!("Could not reach Langfuse at {}: {}", clean_host, e))
        })?;

        let status = res.status();
        if status.is_success() {
            Ok(format!(
                "Connection to Langfuse ({}) successful! Status: {}",
                clean_host, status
            ))
        } else {
            let body = res.text().await.unwrap_or_default();
            Err(Trans4mersError::Network(format!(
                "Langfuse responded with HTTP {}: {}",
                status, body
            )))
        }
    }
}

impl Default for LangfuseClient {
    fn default() -> Self {
        Self::new()
    }
}
