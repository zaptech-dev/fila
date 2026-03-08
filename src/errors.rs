use rapina::database::DbError;
use rapina::prelude::*;

pub enum CrudError {
    Db(DbError),
}

impl IntoApiError for CrudError {
    fn into_api_error(self) -> Error {
        match self {
            CrudError::Db(e) => e.into_api_error(),
        }
    }
}

impl DocumentedError for CrudError {
    fn error_variants() -> Vec<ErrorVariant> {
        vec![
            ErrorVariant {
                status: 404,
                code: "NOT_FOUND",
                description: "Resource not found",
            },
            ErrorVariant {
                status: 500,
                code: "DATABASE_ERROR",
                description: "Database operation failed",
            },
        ]
    }
}

impl From<DbError> for CrudError {
    fn from(e: DbError) -> Self {
        CrudError::Db(e)
    }
}
