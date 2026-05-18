use actix_web::{
    HttpResponse, ResponseError,
    http::{StatusCode, header::ContentType},
};
use derive_more::Display;
use maud::Markup;

use crate::server::views::html::error::{render, render_404};

/// HTTP 层统一错误类型，用于把内部失败映射成用户可读的状态页。
#[derive(Debug, Display)]
pub(crate) enum ServerError {
    #[display("Could not retrieve popular items")]
    PopularItemsFailed,

    #[display("Crate not found")]
    CrateNotFound,

    #[display("Could not parse crate path")]
    BadCratePath,

    #[display("Could not fetch crate information")]
    CrateFetchFailed,

    #[display("Could not parse repository path")]
    BadRepoPath,

    /// repo 路径合法但依赖分析失败，用于避免把分析失败误报成路径错误。
    #[display("Could not analyze repository")]
    RepoAnalysisFailed,

    #[display("Crate/repo analysis failed")]
    AnalysisFailed(Markup),
}

impl ResponseError for ServerError {
    fn status_code(&self) -> StatusCode {
        match self {
            ServerError::PopularItemsFailed => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::CrateNotFound => StatusCode::NOT_FOUND,
            ServerError::BadCratePath => StatusCode::BAD_REQUEST,
            ServerError::CrateFetchFailed => StatusCode::NOT_FOUND,
            ServerError::BadRepoPath => StatusCode::BAD_REQUEST,
            ServerError::RepoAnalysisFailed => StatusCode::BAD_REQUEST,
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

            ServerError::CrateFetchFailed => res.body(
                render(
                    self.to_string(),
                    "Please make sure to provide a valid crate name.",
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

            ServerError::RepoAnalysisFailed => res.body(
                render(
                    self.to_string(),
                    "The repository you requested might be structured in an uncommon way that is not yet supported.",
                )
                .0,
            ),

            Self::AnalysisFailed(html) => res.body(html.0.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{ResponseError, http::StatusCode};

    use super::ServerError;

    #[test]
    fn repo_analysis_failure_is_not_reported_as_bad_path() {
        assert_eq!(
            ServerError::RepoAnalysisFailed.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ServerError::RepoAnalysisFailed.to_string(),
            "Could not analyze repository"
        );
    }
}
