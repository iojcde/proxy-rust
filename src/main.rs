use hyper::service::{make_service_fn, service_fn};
mod errors;
mod listener;
mod ssl;
use errors::send_error_res;
use hyper::{client, Body, Request, Response};
use listener::Incoming;
use listener::{Connection, Listener};

type GenericError = Box<dyn std::error::Error + Send + Sync>;

fn main() {
  // Serve an echo service over HTTPS, with proper error handling.
  if let Err(e) = run_server() {
    println!("FAILED: {}", e);
  }
}

#[tokio::main]
async fn run_server() -> Result<(), GenericError> {
  pretty_env_logger::init();

  let in_addr = format!("127.0.0.1:{}", 1337);

  // The closure inside `make_service_fn` is run for each connection,
  // creating a 'service' to handle requests for that specific connection.

  // Create a TCP listener via tokio.
  let mut resolver = rustls::ResolvesServerCertUsingSNI::new();
  ssl::add_certificate_to_resolver("localhost", &mut resolver);
  let listener = ssl::bind_tls(in_addr.parse().unwrap(), resolver).await?;
  // Prepare a long-running future stream to accept and serve clients.
  Ok(http_server(listener).await?)
}

async fn http_server<L>(listener: L) -> Result<(), hyper::Error>
where
  L: Listener + Send,
  <L as Listener>::Connection: Send + Unpin + 'static,
{
  let https = {
    // Build an HTTP connector which supports HTTPS too.
    let mut http = client::HttpConnector::new();
    http.enforce_http(true);
    let tls = rustls::ClientConfig::new();

    // Join the above part into an HTTPS connector.
    hyper_rustls::HttpsConnector::from((http, tls));
    // Default HTTPS connector.
    hyper_rustls::HttpsConnector::with_native_roots()
  };
  let client: client::Client<_, hyper::Body> = hyper::client::Client::builder().build(https);
  let service = make_service_fn(move |s: &L::Connection| {
    let client = client.clone();
    let ip = s.remote_addr();

    let sni_hostname = s.sni_hostname().map(|name| name.to_string()).unwrap();

    async move {
      Ok::<_, GenericError>(service_fn(move |req: Request<Body>| {
        let sni_hostname = sni_hostname.clone();
        handle(req, ip, client.to_owned(), sni_hostname)
      }))
    }
  });
  let server = hyper::Server::builder(Incoming::new(listener))
    .http1_preserve_header_case(false)
    .serve(service);
  server.await
}

//<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> hyper::Client<hyper::client::HttpConnector>,
async fn proxy(
  mut req: Request<Body>,
  client: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
) -> Result<Response<Body>, GenericError> {
  let out_addr = "http://localhost:8000";

  let uri_string = format!(
    "{}{}",
    out_addr,
    req
      .uri()
      .path_and_query()
      .map(|x| x.as_str())
      .unwrap_or("/")
  )
  .to_owned();
  *req.version_mut() = hyper::Version::HTTP_11;
  *req.uri_mut() = uri_string.parse()?;
  let forward_res = client.request(req).await.unwrap_or_else(move |_req| {
    send_error_res(
      "502 Bad Gateway: Origin Server Down".to_string(),
      http::StatusCode::BAD_GATEWAY,
    )
    .unwrap()
  });
  // let (parts, body) = forward_res.into_parts();
  //let body = hyper::body::to_bytes(body).await.unwrap().to_vec();
  //let final_res = Response::from_parts(parts, hyper::Body::from(body));
  Ok(forward_res)
}

async fn handle(
  req: Request<Body>,
  ip: std::net::SocketAddr,
  client: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
  sni_hostname: String,
) -> Result<Response<Body>, http::Error> {
  if None == req.headers().get("host") && None == req.uri().authority() {
    return send_error_res("Bad Request".to_string(), http::StatusCode::BAD_REQUEST);
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
  Ok(
    proxy(Request::from_parts(parts, body), client.to_owned())
      .await
      .unwrap(),
  )
}
