use crate::ClientType;
use crate::{errors::Error, send_error_res, uri::UriExt};
use bytes::Bytes;
use hyper::{header, Body, HeaderMap, Method, Request, Response, StatusCode, Uri};

pub fn remove_sensitive_headers(headers: &mut HeaderMap, next: &Uri, previous: &Uri) {
    if !next.is_same_host(previous) {
        headers.remove("authorization");
        headers.remove("cookie");
        headers.remove("cookie2");
        headers.remove("www-authenticate");
    }
}

struct State {
    method: Method,
    uri: Uri,
    version: http::Version,
    headers: HeaderMap,
    body: Bytes,
    remaining_redirects: usize,
}

enum Decision {
    Continue,
    Return,
}

impl State {
    pub fn new<B>(req: &mut Request<B>, max_redirects: usize) -> State {
        let mut state = State {
            method: req.method().clone(),
            uri: req.uri().clone(),
            version: req.version(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            remaining_redirects: max_redirects,
        };
        state.headers = ::std::mem::replace(req.headers_mut(), HeaderMap::new());
        state
    }

    pub fn create_request(&self) -> Request<Body> {
        let mut req = Request::builder()
            .uri(self.uri.clone())
            .method(self.method.clone())
            .version(self.version.clone())
            .body(Body::from(self.body.clone()))
            .unwrap();

        req.headers_mut().clone_from(&self.headers);
        req
    }

    pub fn follow_redirect(&mut self, res: &Response<Body>) -> Result<Decision, Error> {
        self.remaining_redirects -= 1;

        if self.remaining_redirects == 0 {
            return Ok(Decision::Return);
        }

        if let Some(location) = res.headers().get(header::LOCATION) {
            let next = self.uri.compute_redirect(location.to_owned())?;
            remove_sensitive_headers(&mut self.headers, &next, &self.uri);
            self.uri = next;

            Ok(Decision::Continue)
        } else {
            Ok(Decision::Return)
        }
    }

    pub fn handle_response(&mut self, res: &Response<Body>) -> Result<Decision, Error> {
        match res.status() {
            StatusCode::MOVED_PERMANENTLY | StatusCode::PERMANENT_REDIRECT => {
                self.follow_redirect(res)
            }
            StatusCode::FOUND | StatusCode::TEMPORARY_REDIRECT => self.follow_redirect(res),
            StatusCode::SEE_OTHER => self.follow_redirect(res),
            _ => Ok(Decision::Return),
        }
    }
}

pub async fn request(req: &mut Request<Body>, client: ClientType) -> Response<Body> {
    let remaining = 10;

    let mut state = State::new(req, 10);

    let mut res = client
        .request(state.create_request())
        .await
        .unwrap_or_else(move |err| {
            if err.is_closed() {
                send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
            } else if err.is_connect() {
                send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
            } else {
                send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
            }
        });

    while remaining > 0 {
        let client = client.clone();
        match state.handle_response(&res).unwrap_or(Decision::Continue) {
            Decision::Continue => {
                res = client
                    .request(state.create_request())
                    .await
                    .unwrap_or_else(move |err| {
                        if err.is_closed() {
                            send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
                        } else if err.is_connect() {
                            send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
                        } else {
                            send_error_res(http::StatusCode::BAD_GATEWAY).unwrap()
                        }
                    });
            }
            Decision::Return => return res,
        }
    }

    return res;
}
