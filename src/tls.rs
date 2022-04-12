use crate::listener::{Connection, Listener};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::sign::{RSASigningKey, SigningKey};
use rustls::ResolvesServerCertUsingSNI;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{server::TlsStream, Accept, TlsAcceptor};

use futures_util::Future;

use std::sync::Arc;

pub fn add_certificate_to_resolver(hostname: &str, resolver: &mut ResolvesServerCertUsingSNI) {
  //let resolve = |filename| format!("./{filename}", filename = &filename);
  //    config_dir = env::var("XDG_CONFIG_HOME").unwrap().to_string(),

  let cert_file = &mut BufReader::new(File::open("cert.pem").unwrap());
  let key_file = &mut BufReader::new(File::open("privkey.pem").unwrap());

  let cert_chain = certs(cert_file).unwrap();
  let mut keys = pkcs8_private_keys(key_file).unwrap();
  let signing_key = RSASigningKey::new(&keys.remove(0)).unwrap();
  let signing_key_boxed: Arc<Box<dyn SigningKey>> = Arc::new(Box::new(signing_key));

  resolver
    .add(
      hostname,
      rustls::sign::CertifiedKey::new(cert_chain, signing_key_boxed),
    )
    .expect("Invalid certificate");
}

/* pub fn init_certs(configs: Vec<config::ConfigItem>) {
  let existing: Vec<config::ConfigItem> = configs
    .into_iter()
    .filter(|host| {
      Path::new(&format!(
        "{config_dir}/proxy/{domain}",
        config_dir = env::var("XDG_CONFIG_HOME").unwrap().to_string(),
        domain = &host.domain
      ))
      .exists()
    })
    .collect();
}V
 */

pub struct TlsListener {
  listener: TcpListener,
  acceptor: TlsAcceptor,
  state: TlsListenerState,
}

enum TlsListenerState {
  Listening,
  Accepting(Accept<TcpStream>),
}

impl Listener for TlsListener {
  type Connection = TlsStream<TcpStream>;

  fn local_addr(&self) -> Option<SocketAddr> {
    self.listener.local_addr().ok()
  }

  fn poll_accept(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<Self::Connection>> {
    loop {
      match self.state {
        TlsListenerState::Listening => match self.listener.poll_accept(cx) {
          Poll::Pending => return Poll::Pending,
          Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
          Poll::Ready(Ok((stream, _addr))) => {
            let fut = self.acceptor.accept(stream);
            self.state = TlsListenerState::Accepting(fut);
          }
        },
        TlsListenerState::Accepting(ref mut fut) => match Pin::new(fut).poll(cx) {
          Poll::Pending => return Poll::Pending,
          Poll::Ready(result) => {
            self.state = TlsListenerState::Listening;
            return Poll::Ready(result);
          }
        },
      }
    }
  }
}

pub async fn bind_tls(
  address: SocketAddr,
  resolver: rustls::ResolvesServerCertUsingSNI,
) -> io::Result<TlsListener> {
  let listener = TcpListener::bind(address).await?;

  let tls_cfg = {
    // Load public certificate.
    // Do not use client certificate authentication.
    let mut cfg = rustls::ServerConfig::new(rustls::NoClientAuth::new());
    // Select a certificate to use.
    cfg.cert_resolver = Arc::new(resolver);

    cfg.ticketer = rustls::Ticketer::new();
    let cache = rustls::ServerSessionMemoryCache::new(1024);
    cfg.set_persistence(cache);
    // Configure ALPN to accept HTTP/2, HTTP/1.1 in that order.
    cfg.set_protocols(&[b"h2".to_vec(), b"http/1.1".to_vec()]);
    cfg
  };
  
  let acceptor = TlsAcceptor::from(Arc::new(tls_cfg));
  let state = TlsListenerState::Listening;

  Ok(TlsListener {
    listener,
    acceptor,
    state,
  })
}

impl Connection for TlsStream<TcpStream> {
  fn remote_addr(&self) -> SocketAddr {
    self.get_ref().0.peer_addr().unwrap()
  }
  fn sni_hostname(&self) -> Option<&str> {
    self.get_ref().1.get_sni_hostname()
  }
}
