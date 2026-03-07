use rapina::database::DbError;
use rapina::prelude::*;

pub enum BatchError {
    DbError(DbError),
}

impl IntoApiError for BatchError {
    fn into_api_error(self) -> Error {
        match self {
            BatchError::DbError(e) => e.into_api_error(),
        }
    }
}

impl DocumentedError for BatchError {
    fn error_variants() -> Vec<ErrorVariant> {
        vec![
            ErrorVariant {
                status: 404,
                code: "NOT_FOUND",
                description: "Batch not found",
            },
            ErrorVariant {
                status: 500,
                code: "DATABASE_ERROR",
                description: "Database operation failed",
            },
        ]
    }
}

impl From<DbError> for BatchError {
    fn from(e: DbError) -> Self {
        BatchError::DbError(e)
    }
}
