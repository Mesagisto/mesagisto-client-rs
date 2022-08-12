use std::io::BufReader;

use color_eyre::eyre::Result;
use quinn::ClientConfig;
use rustls::RootCertStore;

pub fn read_certs_from_file() -> Result<Vec<rustls::Certificate>> {
  let mut cert_chain_reader = BufReader::new(std::fs::File::open("./res/server-cert.pem")?);
  let certs = rustls_pemfile::certs(&mut cert_chain_reader)?
    .into_iter()
    .map(rustls::Certificate)
    .collect::<Vec<_>>();
  assert!(!certs.is_empty());

  Ok(certs)
}

pub async fn client_config() -> Result<ClientConfig> {
  let cert_store = tokio::task::spawn_blocking(|| {
    fn inner() -> Result<RootCertStore> {
      let certs = read_certs_from_file()?;
      let mut cert_store = rustls::RootCertStore::empty();
      for cert in certs {
        cert_store.add(&cert)?;
      }
      Ok(cert_store)
    }
    inner()
  })
  .await??;
  let client_config = quinn::ClientConfig::with_root_certificates(cert_store);
  Ok(client_config)
}
