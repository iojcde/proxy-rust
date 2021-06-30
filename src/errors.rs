use hyper::{Body, Response};

use log::error;
pub fn send_error_res(msg: String, code: http::StatusCode) -> Result<Response<Body>, http::Error> {
  error!("{}", msg);
  Response::builder().status(code).body(Body::from(msg))
}
