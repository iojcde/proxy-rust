use http::{status, StatusCode};
use hyper::{Body, Response};
use log::error;
use std::io;

pub fn send_error_res(code: status::StatusCode) -> Result<Response<Body>, http::Error> {
    let msg = match code {
        StatusCode::BAD_GATEWAY => format!("{}: BAD_GATEWAY", code.as_u16()),
        StatusCode::BAD_REQUEST => format!("{}: BAD_REQUEST", code.as_u16()),
        _ => format!("{}", code).to_uppercase(),
    };

    Response::builder().status(code).body(Body::from(msg))
}

use thiserror::Error;

/// Lib errors wrapper
/// Encapsulates the different errors that can occur during forwarding requests
#[derive(Error, Debug)]
pub enum Error {
    // FIXME: allow warning for now, must be renamed for next breaking api version
    #[allow(clippy::upper_case_acronyms)]
    /// Errors when connecting to the target service
    #[error("Http error: {0}")]
    HTTP(#[from] hyper::http::Error),

    #[error("io error: {0}")]
    IO(#[from] io::Error),

    #[error("invalid uri: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),
}
