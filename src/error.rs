use std::{env::VarError, io};

use axum::response::IntoResponse;
use hyper::StatusCode;
use thiserror::Error;

pub type Result<S> = std::result::Result<S, Error>;
#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing or invalid environment variable: {0}")]
    EnvVar(#[from] VarError),

    #[error("Failed to bind to address: {0}")]
    Io(#[from] io::Error),
}
impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let err = match self {
            Self::EnvVar(_) | Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let msg = self.to_string();
        (err, msg).into_response()
    }
}
