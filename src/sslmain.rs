//! Simple HTTPS echo service based on hyper-rustls
//!
//! First parameter is the mandatory port to use.
//! Certificate and private key are hardcoded to sample files.
//! hyper will automatically use HTTP/2 if a client starts talking HTTP/2,
//! otherwise HTTP/1.1 will be used.
use async_stream::stream;
use std::convert::Infallible;
mod ssl;
use core::task::{Context, Poll};
use futures_util::{future::TryFutureExt, stream::Stream};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use std::pin::Pin;
use std::sync::Arc;
use std::{env, fs, io, sync};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use hyper::body::HttpBody as _;
use tokio::io::{stdout, AsyncWriteExt as _};

fn main() {
  // Serve an echo service over HTTPS, with proper error handling.
  if let Err(e) = run_server() {
    eprintln!("FAILED: {}", e);
    std::process::exit(1);
  }
}

fn error(err: String) -> io::Error {
  io::Error::new(io::ErrorKind::Other, err)
}

#[tokio::main]
async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  // First parameter is port number (optional, defaults to 1337)
  let port = match env::args().nth(1) {
    Some(ref p) => p.to_owned(),
    None => "1337".to_owned(),
  };
  let addr = format!("127.0.0.1:{}", port);

  // Build TLS configuration.
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
  let tcp = TcpListener::bind(&addr).await?;
  let tls_acceptor = TlsAcceptor::from(tls_cfg);
  // Prepare a long-running future stream to accept and serve clients.
  let incoming_tls_stream = stream! {
      loop {
          let (socket, _) = tcp.accept().await?;
          let stream = tls_acceptor.accept(socket).map_err(|e| {
              println!("[!] Voluntary server halt due to client-connection error...");
              error(e.to_string())
          });
          yield stream.await;
      }
  };
  let service = make_service_fn(|_| async { Ok::<_, Infallible>(service_fn(handle)) });
  let server = Server::builder(HyperAcceptor {
    acceptor: Box::pin(incoming_tls_stream),
  })
  .serve(service);

  // Run the future, keep going until an error occurs.
  println!("Starting to serve on https://{}.", addr);
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

// Custom echo service, handling two different routes and a
// catch-all 404 responder.
async fn handle(_: Request<Body>) -> Result<Response<Body>, Infallible> {
  let client = Client::new();

  let mut res = client.get(url).await?;

  println!("Response: {}", res.status());
  println!("Headers: {:#?}\n", res.headers());

  // Stream the body, writing each chunk to stdout as we get it
  // (instead of buffering and printing at the end).
  while let Some(next) = res.data().await {
      let chunk = next?;
      io::stdout().write_all(&chunk).await?;
  }

  Ok(Response::new(resp.))
}
/*
// Load public certificate from file.f
fn load_certs(filename: &str) -> io::Result<Vec<rustls::Certificate>> {
  // Open certificate file.
  let certfile =
    fs::File::open(filename).map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
  let mut reader = io::BufReader::new(certfile);

  // Load and return certificate.
  pemfile::certs(&mut reader).map_err(|_| error("failed to load certificate".into()))
}

// Load private key from file.
fn load_private_key(filename: &str) -> io::Result<rustls::PrivateKey> {
  // Open keyfile.
  let keyfile =
    fs::File::open(filename).map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
  let mut reader = io::BufReader::new(keyfile);

  // Load and return a single private key.
  let keys = pemfile::pkcs8_private_keys(&mut reader)
    .map_err(|_| error("failed to load private key".into()))?;
  if keys.len() != 1 {
    return Err(error("expected a single private key".into()));
  }
  Ok(keys[0].clone())
} */
