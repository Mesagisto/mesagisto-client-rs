use std::{io::BufReader, sync::Arc};

use color_eyre::eyre::Result;
use quinn::{ClientConfig, IdleTimeout, TransportConfig, VarInt};
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
      match rustls_native_certs::load_native_certs() {
        Ok(certs) => {
          for cert in certs {
            if let Err(e) = cert_store.add(&rustls::Certificate(cert.0)) {
              tracing::warn!("failed to parse trust anchor: {}", e);
            }
          }
        }
        Err(e) => {
          tracing::warn!("couldn't load any default trust roots: {}", e);
        }
      };
      Ok(cert_store)
    }
    inner()
  })
  .await??;
  let mut client_config = quinn::ClientConfig::with_root_certificates(cert_store);
  let mut transport = TransportConfig::default();
  transport.max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(15_000))));
  client_config.transport = Arc::new(transport);
  Ok(client_config)
}
