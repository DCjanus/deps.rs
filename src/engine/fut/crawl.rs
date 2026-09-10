use std::fmt;

use anyhow::Error;
use futures_util::{
    FutureExt as _, StreamExt as _, future::LocalBoxFuture, stream::FuturesOrdered,
};
use relative_path::RelativePathBuf;

use crate::{
    engine::{
        Engine,
        machines::crawler::{ManifestCrawler, ManifestCrawlerOutput},
    },
    interactors::RetrieveFileError,
    models::repo::RepoPath,
};

#[derive(Debug)]
pub enum CrawlManifestError {
    EntryNotFound,
    InvalidManifest(Error),
    Upstream(Error),
}

impl fmt::Display for CrawlManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntryNotFound => f.write_str("entry Cargo.toml not found"),
            Self::InvalidManifest(err) => write!(f, "repository manifest is invalid: {err}"),
            Self::Upstream(err) => write!(f, "repository source is unavailable: {err}"),
        }
    }
}

impl std::error::Error for CrawlManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EntryNotFound => None,
            Self::InvalidManifest(err) | Self::Upstream(err) => Some(err.as_ref()),
        }
    }
}

fn entry_error(err: RetrieveFileError) -> CrawlManifestError {
    match err {
        RetrieveFileError::NotFound(_) => CrawlManifestError::EntryNotFound,
        RetrieveFileError::Rejected { .. } => CrawlManifestError::InvalidManifest(err.into()),
        RetrieveFileError::Unavailable(_) => CrawlManifestError::Upstream(err.into()),
    }
}

fn nested_error(err: RetrieveFileError) -> CrawlManifestError {
    match err {
        RetrieveFileError::NotFound(_) | RetrieveFileError::Rejected { .. } => {
            CrawlManifestError::InvalidManifest(err.into())
        }
        RetrieveFileError::Unavailable(_) => CrawlManifestError::Upstream(err.into()),
    }
}

pub async fn crawl_manifest(
    engine: Engine,
    repo_path: RepoPath,
    entry_point: RelativePathBuf,
) -> Result<ManifestCrawlerOutput, CrawlManifestError> {
    let mut crawler = ManifestCrawler::new();
    let mut futures: FuturesOrdered<
        LocalBoxFuture<'static, Result<(RelativePathBuf, String), CrawlManifestError>>,
    > = FuturesOrdered::new();

    let engine2 = engine.clone();
    let repo_path2 = repo_path.clone();

    let fut = async move {
        let contents = engine2
            .retrieve_manifest_at_path(&repo_path2, &entry_point)
            .await
            .map_err(entry_error)?;
        Ok((entry_point, contents))
    }
    .boxed_local();

    futures.push_back(fut);

    while let Some(item) = futures.next().await {
        let (path, raw_manifest) = item?;
        let output = crawler
            .step(path, raw_manifest)
            .map_err(CrawlManifestError::InvalidManifest)?;

        let engine = engine.clone();
        let repo_path = repo_path.clone();

        for path in output.paths_of_interest {
            let engine = engine.clone();
            let repo_path = repo_path.clone();

            let fut = async move {
                let contents = engine
                    .retrieve_manifest_at_path(&repo_path, &path)
                    .await
                    .map_err(nested_error)?;
                Ok((path, contents))
            }
            .boxed_local();

            futures.push_back(fut);
        }
    }

    Ok(crawler.finalize())
}
