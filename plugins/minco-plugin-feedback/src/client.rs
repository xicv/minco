use crate::{
    DeveloperReplyInput, FeedbackId, FeedbackListFilter, FeedbackMutationResult, FeedbackStatus,
    FeedbackSummary, FeedbackThread, TransitionFeedbackInput,
};
use reqwest::{StatusCode, Url};
use serde::de::DeserializeOwned;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct FeedbackApiClient {
    client: reqwest::Client,
    base_url: Url,
    developer_token: String,
}

impl std::fmt::Debug for FeedbackApiClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FeedbackApiClient")
            .field("base_url", &self.base_url)
            .field("developer_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl FeedbackApiClient {
    pub fn new(
        base_url: impl AsRef<str>,
        developer_token: impl Into<String>,
    ) -> Result<Self, FeedbackApiClientError> {
        let mut base_url = Url::parse(base_url.as_ref())
            .map_err(|error| FeedbackApiClientError::Url(error.to_string()))?;
        if !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(FeedbackApiClientError::Configuration(
                "feedback API URL must not contain credentials, a query, or a fragment".into(),
            ));
        }
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        let developer_token = developer_token.into();
        if developer_token.trim().is_empty() {
            return Err(FeedbackApiClientError::Configuration(
                "developer token must not be empty".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| FeedbackApiClientError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            base_url,
            developer_token,
        })
    }

    pub async fn inbox(
        &self,
        filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackApiClientError> {
        let mut url = self.endpoint("developer/threads")?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(status) = filter.status {
                query.append_pair("status", &status.to_string());
            }
            if let Some(project_id) = filter.project_id {
                query.append_pair("project_id", &project_id);
            }
            query.append_pair("limit", &filter.limit.clamp(1, 200).to_string());
        }
        self.send_json(self.client.get(url)).await
    }

    pub async fn get(&self, id: FeedbackId) -> Result<FeedbackThread, FeedbackApiClientError> {
        let url = self.endpoint(&format!("developer/threads/{id}"))?;
        self.send_json(self.client.get(url)).await
    }

    pub async fn reply(
        &self,
        id: FeedbackId,
        input: DeveloperReplyInput,
    ) -> Result<FeedbackMutationResult, FeedbackApiClientError> {
        let url = self.endpoint(&format!("developer/threads/{id}/messages"))?;
        self.send_json(self.client.post(url).json(&input)).await
    }

    pub async fn transition(
        &self,
        id: FeedbackId,
        status: FeedbackStatus,
        resolution: Option<String>,
        author_display: Option<String>,
    ) -> Result<FeedbackMutationResult, FeedbackApiClientError> {
        let url = self.endpoint(&format!("developer/threads/{id}/status"))?;
        self.send_json(self.client.patch(url).json(&TransitionFeedbackInput {
            status,
            resolution,
            author_display,
        }))
        .await
    }

    pub async fn ai_context_markdown(
        &self,
        id: FeedbackId,
    ) -> Result<String, FeedbackApiClientError> {
        let url = self.endpoint(&format!("developer/threads/{id}/ai-context"))?;
        let response = self.authorized(self.client.get(url)).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(response_error(status, response).await);
        }
        response
            .text()
            .await
            .map_err(|error| FeedbackApiClientError::Transport(error.to_string()))
    }

    pub async fn attachment(
        &self,
        id: FeedbackId,
        attachment_id: Uuid,
    ) -> Result<Vec<u8>, FeedbackApiClientError> {
        let url = self.endpoint(&format!(
            "developer/threads/{id}/attachments/{attachment_id}"
        ))?;
        let response = self.authorized(self.client.get(url)).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(response_error(status, response).await);
        }
        response
            .bytes()
            .await
            .map(|value| value.to_vec())
            .map_err(|error| FeedbackApiClientError::Transport(error.to_string()))
    }

    fn endpoint(&self, path: &str) -> Result<Url, FeedbackApiClientError> {
        self.base_url
            .join(path)
            .map_err(|error| FeedbackApiClientError::Url(error.to_string()))
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.bearer_auth(&self.developer_token)
    }

    async fn send_json<T>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, FeedbackApiClientError>
    where
        T: DeserializeOwned,
    {
        let response = self.authorized(request).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(response_error(status, response).await);
        }
        response
            .json::<T>()
            .await
            .map_err(|error| FeedbackApiClientError::Protocol(error.to_string()))
    }
}

async fn response_error(status: StatusCode, response: reqwest::Response) -> FeedbackApiClientError {
    let detail = match response.json::<serde_json::Value>().await {
        Ok(value) => value
            .get("detail")
            .or_else(|| value.get("title"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("feedback API request failed")
            .to_owned(),
        Err(_) => "feedback API request failed".into(),
    };
    FeedbackApiClientError::Remote { status, detail }
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackApiClientError {
    #[error("feedback client configuration is invalid: {0}")]
    Configuration(String),
    #[error("invalid feedback API URL: {0}")]
    Url(String),
    #[error("feedback API transport failed: {0}")]
    Transport(String),
    #[error("feedback API returned an invalid response: {0}")]
    Protocol(String),
    #[error("feedback API returned {status}: {detail}")]
    Remote { status: StatusCode, detail: String },
}

impl From<reqwest::Error> for FeedbackApiClientError {
    fn from(value: reqwest::Error) -> Self {
        Self::Transport(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_requires_a_nonempty_developer_token() {
        assert!(FeedbackApiClient::new("https://example.test/_minco/feedback/", "").is_err());
    }

    #[test]
    fn client_rejects_base_urls_that_can_expose_credentials_or_change_routing() {
        for base_url in [
            "https://user:password@example.test/_minco/feedback/",
            "https://example.test/_minco/feedback/?token=secret",
            "https://example.test/_minco/feedback/#developer",
        ] {
            assert!(
                FeedbackApiClient::new(base_url, "developer-token").is_err(),
                "{base_url}"
            );
        }
    }

    #[test]
    fn client_normalizes_the_base_url_for_relative_endpoints() {
        let client =
            FeedbackApiClient::new("https://example.test/_minco/feedback", "developer-token")
                .unwrap();
        assert_eq!(
            client.endpoint("developer/threads").unwrap().as_str(),
            "https://example.test/_minco/feedback/developer/threads"
        );
    }
}
