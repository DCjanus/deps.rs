use actix_web::{
    HttpResponse, ResponseError,
    http::{StatusCode, header::ContentType},
};
use derive_more::Display;
use maud::Markup;

use crate::server::views::html::error::{render, render_404};

#[derive(Debug, Display)]
pub(crate) enum ServerError {
    #[display("Could not retrieve popular items")]
    PopularItemsFailed,

    #[display("Crate not found")]
    CrateNotFound,

    #[display("Could not parse crate path")]
    BadCratePath,

    #[display("Could not parse repository path")]
    BadRepoPath,

    #[display("Dependency analysis is temporarily unavailable")]
    AnalysisUnavailable,

    #[display("Repository manifest not found")]
    RepoNotFound,

    #[display("Repository manifest could not be analyzed")]
    RepoManifestInvalid,

    #[display("Repository source is temporarily unavailable")]
    RepoUpstreamUnavailable,

    #[display("Crate/repo analysis failed")]
    AnalysisFailed(Markup),
}

impl ResponseError for ServerError {
    fn status_code(&self) -> StatusCode {
        match self {
            ServerError::PopularItemsFailed => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::CrateNotFound => StatusCode::NOT_FOUND,
            ServerError::BadCratePath => StatusCode::BAD_REQUEST,
            ServerError::BadRepoPath => StatusCode::BAD_REQUEST,
            ServerError::AnalysisUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            ServerError::RepoNotFound => StatusCode::NOT_FOUND,
            ServerError::RepoManifestInvalid => StatusCode::UNPROCESSABLE_ENTITY,
            ServerError::RepoUpstreamUnavailable => StatusCode::BAD_GATEWAY,
            ServerError::AnalysisFailed(_) => StatusCode::BAD_REQUEST,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let mut res = HttpResponse::build(self.status_code());
        let res = res.insert_header(ContentType::html());

        match self {
            ServerError::PopularItemsFailed => res.body(render(self.to_string(), "").0),

            ServerError::CrateNotFound => res.body(render_404().0),

            ServerError::BadCratePath => res.body(
                render(
                    self.to_string(),
                    "Please make sure to provide a valid crate name and version.",
                )
                .0,
            ),

            ServerError::BadRepoPath => res.body(
                render(
                    self.to_string(),
                    "Please make sure to provide a valid repository path.",
                )
                .0,
            ),

            ServerError::AnalysisUnavailable => {
                res.body(render(self.to_string(), "Please try again later.").0)
            }

            ServerError::RepoNotFound => res.body(render_404().0),

            ServerError::RepoManifestInvalid => res.body(
                render(
                    self.to_string(),
                    "Please check the repository path and Cargo manifests.",
                )
                .0,
            ),

            ServerError::RepoUpstreamUnavailable => {
                res.body(render(self.to_string(), "Please try again later.").0)
            }

            Self::AnalysisFailed(html) => res.body(html.0.clone()),
        }
    }
}
