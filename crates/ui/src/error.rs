use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use kameo::error::SendError;
use maud::{html, Markup};

use umari_runtime::module_store::ModuleStoreError;

pub struct HtmlError {
    pub status: StatusCode,
    pub message: String,
}

impl HtmlError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        HtmlError {
            status,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        HtmlError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        HtmlError::new(StatusCode::NOT_FOUND, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        HtmlError::new(StatusCode::BAD_REQUEST, message)
    }

    fn fragment(&self) -> Markup {
        html! {
            article.error {
                strong { "Error" }
                p { (self.message) }
            }
        }
    }
}

impl IntoResponse for HtmlError {
    fn into_response(self) -> Response {
        (self.status, self.fragment()).into_response()
    }
}

impl From<ModuleStoreError> for HtmlError {
    fn from(err: ModuleStoreError) -> Self {
        let status = match &err {
            ModuleStoreError::ModuleNotFound { .. } => StatusCode::NOT_FOUND,
            ModuleStoreError::ModuleAlreadyExists => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        HtmlError::new(status, err.to_string())
    }
}

impl<M, E> From<SendError<M, E>> for HtmlError
where
    E: Into<HtmlError> + std::fmt::Display,
{
    fn from(err: SendError<M, E>) -> Self {
        match err {
            SendError::HandlerError(err) => err.into(),
            _ => HtmlError::internal(err.to_string()),
        }
    }
}

impl From<umari_runtime::command::CommandError> for HtmlError {
    fn from(err: umari_runtime::command::CommandError) -> Self {
        use umari_runtime::command::CommandError;
        let status = match &err {
            CommandError::ModuleNotFound { .. } => StatusCode::NOT_FOUND,
            CommandError::SerializeInput { .. }
            | CommandError::CommandHandler { .. } => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        HtmlError::new(status, err.to_string())
    }
}
