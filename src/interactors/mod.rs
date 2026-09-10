use std::fmt;

use actix_web::dev::Service;
use anyhow::Error;
use futures_util::{FutureExt as _, future::LocalBoxFuture};
use relative_path::RelativePathBuf;

use crate::models::repo::RepoPath;

pub mod crates;
pub mod github;
pub mod rustsec;

#[derive(Debug)]
pub enum RetrieveFileError {
    NotFound(String),
    Rejected {
        status: reqwest::StatusCode,
        url: String,
    },
    Unavailable(Error),
}

impl fmt::Display for RetrieveFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(url) => write!(f, "file not found at {url}"),
            Self::Rejected { status, url } => {
                write!(f, "upstream returned {status} for {url}")
            }
            Self::Unavailable(err) => write!(f, "could not retrieve file: {err}"),
        }
    }
}

impl std::error::Error for RetrieveFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unavailable(err) => Some(err.as_ref()),
            Self::NotFound(_) | Self::Rejected { .. } => None,
        }
    }
}

#[derive(Clone)]
pub struct RetrieveFileAtPath {
    client: reqwest::Client,
}

impl RetrieveFileAtPath {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn query(
        client: reqwest::Client,
        repo_path: RepoPath,
        path: RelativePathBuf,
    ) -> Result<String, RetrieveFileError> {
        let url = repo_path.to_usercontent_file_url(&path);
        let res = client
            .get(&url)
            .send()
            .await
            .map_err(|err| RetrieveFileError::Unavailable(err.into()))?;

        match res.status() {
            status if status.is_success() => {}
            reqwest::StatusCode::NOT_FOUND => return Err(RetrieveFileError::NotFound(url)),
            status if status.is_client_error() && !is_retryable_status(status) => {
                return Err(RetrieveFileError::Rejected { status, url });
            }
            status => {
                return Err(RetrieveFileError::Unavailable(anyhow::anyhow!(
                    "upstream returned {status} for {url}"
                )));
            }
        }

        res.text()
            .await
            .map_err(|err| RetrieveFileError::Unavailable(err.into()))
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_EARLY
            | reqwest::StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

impl Service<(RepoPath, RelativePathBuf)> for RetrieveFileAtPath {
    type Response = String;
    type Error = RetrieveFileError;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_web::dev::always_ready!();

    fn call(&self, (repo_path, path): (RepoPath, RelativePathBuf)) -> Self::Future {
        let client = self.client.clone();
        Self::query(client, repo_path, path).boxed()
    }
}

impl fmt::Debug for RetrieveFileAtPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RetrieveFileAtPath")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_http_statuses_include_transient_client_responses() {
        for status in [408, 425, 429, 500, 503] {
            assert!(is_retryable_status(
                reqwest::StatusCode::from_u16(status).unwrap()
            ));
        }

        for status in [400, 401, 403, 404, 422] {
            assert!(!is_retryable_status(
                reqwest::StatusCode::from_u16(status).unwrap()
            ));
        }
    }
}
