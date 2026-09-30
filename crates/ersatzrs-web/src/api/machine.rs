//! Typed machine errors; browser actions keep their own response contracts.
use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use ersatzrs_app::ApplicationError;

pub struct MachineError {
    status: StatusCode,
    message: String,
}

impl From<ApplicationError> for MachineError {
    fn from(error: ApplicationError) -> Self {
        let status = match &error {
            ApplicationError::Validation { .. } => StatusCode::BAD_REQUEST,
            ApplicationError::NotFound { .. } => StatusCode::NOT_FOUND,
            ApplicationError::Conflict(_) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("machine API operation failed");
            "Operation failed".into()
        } else {
            error.to_string()
        };
        Self { status, message }
    }
}

impl From<JsonRejection> for MachineError {
    fn from(error: JsonRejection) -> Self {
        Self {
            status: if error.status() == StatusCode::UNPROCESSABLE_ENTITY {
                StatusCode::BAD_REQUEST
            } else {
                error.status()
            },
            message: error.body_text(),
        }
    }
}

impl IntoResponse for MachineError {
    fn into_response(self) -> Response {
        (self.status, Json(self.message)).into_response()
    }
}

pub fn parse_json<T>(input: Result<Json<T>, JsonRejection>) -> Result<T, MachineError> {
    Ok(input?.0)
}
