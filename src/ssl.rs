use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::sign::{RSASigningKey, SigningKey};
use rustls::ResolvesServerCertUsingSNI;

use std::fs::File;
use std::io::BufReader;

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
