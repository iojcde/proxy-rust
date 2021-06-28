use async_stream::stream;
use hyper::service::{make_service_fn, service_fn};
mod ssl;
use core::task::{Context, Poll};
use futures_util::stream::Stream;
use http::HeaderValue;
use hyper::{client, Body, Request, Response, Server};
use std::pin::Pin;
use std::sync::Arc;
use std::{io, sync};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

type GenericError = Box<dyn std::error::Error + Send + Sync>;

fn main() {
  // Serve an echo service over HTTPS, with proper error handling.
  if let Err(e) = run_server() {
    println!("FAILED: {}", e);
  }
}
fn error(err: String) -> io::Error {
  io::Error::new(io::ErrorKind::Other, err)
}

#[tokio::main]
async fn run_server() -> Result<(), GenericError> {
  pretty_env_logger::init();
  let https = {
    // Build an HTTP connector which supports HTTPS too.
    let mut http = client::HttpConnector::new();
    http.enforce_http(false);
    let tls = rustls::ClientConfig::new();

    // Join the above part into an HTTPS connector.
    hyper_rustls::HttpsConnector::from((http, tls));
    // Default HTTPS connector.
    hyper_rustls::HttpsConnector::with_native_roots()
  };

  let in_addr = format!("127.0.0.1:{}", 1337);

  // The closure inside `make_service_fn` is run for each connection,
  // creating a 'service' to handle requests for that specific connection.

  let tls_cfg = {
    // Load public certificate.
    // Do not use client certificate authentication.
    let mut resolver = rustls::ResolvesServerCertUsingSNI::new();
    ssl::add_certificate_to_resolver("localhost", &mut resolver);
    let mut cfg = rustls::ServerConfig::new(rustls::NoClientAuth::new());
    // Select a certificate to use.
    cfg.cert_resolver = Arc::new(resolver);
    // Configure ALPN to accept HTTP/2, HTTP/1.1 in that order.
    cfg.set_protocols(&[b"h2".to_vec(), b"http/1.1".to_vec()]);
    sync::Arc::new(cfg)
  };

  // Create a TCP listener via tokio.
  let tcp = TcpListener::bind(&in_addr).await?;
  let tls_acceptor = TlsAcceptor::from(tls_cfg);
  // Prepare a long-running future stream to accept and serve clients.
  let incoming_tls_stream = stream! {
      loop {
          let (socket, _) = tcp.accept().await?;
          let stream = tls_acceptor.accept(socket);
          yield stream.await;
      }
  };
  let client: client::Client<_, hyper::Body> = hyper::client::Client::builder().build(https);

  let service = make_service_fn(
    move |s: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>| {
      let client = client.clone();
      let (tls_stream, server_session) = s.get_ref();
      let a = tls_stream.peer_addr().unwrap();
      async move {
        let asdf = a.clone();
        Ok::<_, io::Error>(service_fn(move |req: Request<Body>| {
          let (mut parts, body) = req.into_parts();
          parts.headers.remove("host");
          parts.headers.append(
            "host",
            http::HeaderValue::from_str(&format!("{}", parts.uri.authority().unwrap())).unwrap(),
          );
          parts.headers.remove("x-forwarded-for");
          parts.headers.append(
            "x-forwarded-for",
            http::HeaderValue::from_str(&format!("{}", asdf)).unwrap(),
          );
          let request: Request<Body> = Request::from_parts(parts, body);
          proxy(request, client.to_owned())
        }))
      }
    },
  );

  let server = Server::builder(HyperAcceptor {
    acceptor: Box::pin(incoming_tls_stream),
  })
  .serve(service);
  server.await?;

  Ok(())
}
struct HyperAcceptor<'a> {
  acceptor: Pin<Box<dyn Stream<Item = Result<TlsStream<TcpStream>, io::Error>> + 'a>>,
}

impl hyper::server::accept::Accept for HyperAcceptor<'_> {
  type Conn = TlsStream<TcpStream>;
  type Error = io::Error;

  fn poll_accept(
    mut self: Pin<&mut Self>,
    cx: &mut Context,
  ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
    Pin::new(&mut self.acceptor).poll_next(cx)
  }
}

async fn proxy(
  req: Request<Body>,
  client: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
) -> Result<Response<Body>, GenericError> {
  // Prepare the HTTPS connector.
  let out_addr = "https://jcde.xyz";

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
  let (mut parts, body) = req.into_parts();
  parts.headers.remove("host");
  //   parts.headers.append(
  // "x-forwarded-for",
  //  HeaderValue::from_str(&format!("{}", remote_addr)).unwrap(),
  // );
  let mut request: Request<Body> = Request::from_parts(parts, body);
  *request.uri_mut() = uri_string.parse().unwrap();
  let forward_res = client.request(request);
  Ok(forward_res.await.unwrap())
}
