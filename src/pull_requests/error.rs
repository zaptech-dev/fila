use rapina::database::DbError;
use rapina::prelude::*;

pub enum PullRequestError {
    DbError(DbError),
}

impl IntoApiError for PullRequestError {
    fn into_api_error(self) -> Error {
        match self {
            PullRequestError::DbError(e) => e.into_api_error(),
        }
    }
}

impl DocumentedError for PullRequestError {
    fn error_variants() -> Vec<ErrorVariant> {
        vec![
            ErrorVariant {
                status: 404,
                code: "NOT_FOUND",
                description: "PullRequest not found",
            },
            ErrorVariant {
                status: 500,
                code: "DATABASE_ERROR",
                description: "Database operation failed",
            },
        ]
    }
}

impl From<DbError> for PullRequestError {
    fn from(e: DbError) -> Self {
        PullRequestError::DbError(e)
    }
}
