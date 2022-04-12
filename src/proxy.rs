use crate::{follow_redirects::request, ClientType, GenericError,send_error_res};
use http::uri::Port;
use hyper::{
    header::{self, HeaderValue},
    Body, Request, Response, Uri,
};

pub fn get_non_default_port(uri: &Uri) -> Option<Port<&str>> {
    match (uri.port().map(|p| p.as_u16()), is_schema_secure(uri)) {
        (Some(443), true) => None,
        (Some(80), false) => None,
        _ => uri.port(),
    }
}

fn is_schema_secure(uri: &Uri) -> bool {
    uri.scheme_str()
        .map(|scheme_str| matches!(scheme_str, "wss" | "https"))
        .unwrap_or_default()
}

//<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> hyper::Client<hyper::client::HttpConnector>,
pub async fn proxy(
    mut req: Request<Body>,
    client: ClientType,
) -> Result<Response<Body>, GenericError> {
    let out_addr = "http://a";

    let uri_string = format!(
        "{}{}",
        out_addr,
        req.uri()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
    )
    .to_owned();

    *req.version_mut() = hyper::Version::HTTP_11;
    *req.uri_mut() = uri_string.parse()?;

    let uri = req.uri().clone();

    req.headers_mut().insert(header::HOST, {
        let hostname = uri.host().expect("authority implies host");
        if let Some(port) = get_non_default_port(&uri) {
            let s = format!("{}:{}", hostname, port);
            HeaderValue::from_str(&s)
        } else {
            HeaderValue::from_str(hostname)
        }
        .expect("uri host is valid header value")
    });

    // let mut forward_res = client.request(req).await.unwrap_or_else(move |err| {
    //     send_error_res(err.to_string(), http::StatusCode::BAD_GATEWAY).unwrap()
    // });

    Ok(request(&mut req, client).await)
}

pub async fn handle(
    req: Request<Body>,
    ip: std::net::SocketAddr,
    client: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    sni_hostname: String,
) -> Result<Response<Body>, http::Error> {
    if None == req.headers().get("host") && None == req.uri().authority() {
        return send_error_res(http::StatusCode::BAD_REQUEST);
    }
    let (mut parts, body) = req.into_parts();
    if parts.uri.authority().is_some() {
        parts.headers.insert(
            "host",
            http::HeaderValue::from_str(&format!("{}", parts.uri.authority().unwrap())).unwrap(),
        );
    }

    parts.headers.insert(
        "x-forwarded-for",
        http::HeaderValue::from_str(&format!("{}", ip)).unwrap(),
    );
    Ok(proxy(Request::from_parts(parts, body), client.to_owned())
        .await
        .unwrap())
}
