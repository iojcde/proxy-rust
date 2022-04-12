use hyper::service::{make_service_fn, service_fn};
mod errors;
mod follow_redirects;
mod listener;
mod proxy;
mod tls;
mod uri;

use errors::send_error_res;
use hyper::{client, Body, Request};
use hyper_rustls::HttpsConnectorBuilder;
use listener::{Connection, Incoming, Listener};

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type ClientType = hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>;
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
    tls::add_certificate_to_resolver("localhost", &mut resolver);
    let listener = tls::bind_tls(in_addr.parse().unwrap(), resolver).await?;

    // Prepare a long-running future stream to accept and serve clients.
    Ok(http_server(listener).await?)
}

async fn http_server<L>(listener: L) -> Result<(), hyper::Error>
where
    L: Listener + Send,
    <L as Listener>::Connection: Send + Unpin + 'static,
{
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let client: client::Client<_, hyper::Body> =
        hyper::Client::builder().set_host(true).build(https);

    let service = make_service_fn(move |s: &L::Connection| {
        let client = client.clone();
        let ip = s.remote_addr();

        let sni_hostname = s.sni_hostname().map(|name| name.to_string()).unwrap();

        async move {
            Ok::<_, GenericError>(service_fn(move |req: Request<Body>| {
                let sni_hostname = sni_hostname.clone();
                proxy::handle(req, ip, client.to_owned(), sni_hostname)
            }))
        }
    });
    let server = hyper::Server::builder(Incoming::new(listener))
        .http1_preserve_header_case(false)
        .serve(service);
    server.await
}
