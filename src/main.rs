use async_stream::stream;
use hyper::service::{make_service_fn, service_fn};
mod ssl;
use core::task::{Context, Poll};
use futures_util::stream::Stream;
use hyper::{client, header::HeaderValue, Body, Request, Response, Server};

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
    eprintln!("FAILED: {}", e);
    std::process::exit(1);
  }
}

#[tokio::main]
async fn run_server() -> Result<(), GenericError> {
  pretty_env_logger::init();
  let https = {
    // Build an HTTP connector which supports HTTPS too.
    let mut http = client::HttpConnector::new();
    http.enforce_http(false);
    // Build a TLS client, using the custom CA store for lookups.
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

  let new_service = make_service_fn(
    |s: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>| {
      let (tcp_stream, inner) = s.into_inner();
      let sni_hostname = inner.get_sni_hostname();
      let client = client.clone();
      async {
        Ok::<_, GenericError>(service_fn(move |req: Request<Body>| {
          // Clone again to ensure that client outlives this closure.
          proxy(req, client.to_owned(), tcp_stream.peer_addr().unwrap())
        }))
      }
    },
  );

  let server = Server::builder(HyperAcceptor {
    acceptor: Box::pin(incoming_tls_stream),
  })
  .serve(new_service);
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
  remote_addr: std::net::SocketAddr,
) -> Result<Response<Body>, GenericError> {
  // Prepare the HTTPS connector.
  let out_addr = "https://cloudflare.com";

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
  parts.headers.remove("Host");
  parts.headers.append(
    "x-forwarded-for",
    HeaderValue::from_str(&format!("{}", remote_addr)).unwrap(),
  );
  let mut request: Request<Body> = Request::from_parts(parts, body);
  *request.uri_mut() = uri_string.parse().unwrap();
  let forward_res = client.request(request).await?;
  Ok(forward_res)
}
