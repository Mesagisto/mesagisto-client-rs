use std::{io::BufReader, sync::Arc};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use lateinit::LateInit;
use rustls::{
  client::{ServerCertVerified, ServerCertVerifier},
  RootCertStore,
};

use crate::OkExt;

#[derive(Singleton, Default)]
pub struct Tls {
  pub skip_verify: LateInit<bool>,
  pub custom_cert: LateInit<Option<ArcStr>>,
  pub clinet_config: LateInit<Arc<rustls::ClientConfig>>,
}
impl Tls {
  pub async fn init(&self, skip_verify: bool, custom_cert: Option<ArcStr>) -> Result<()> {
    self.skip_verify.init(skip_verify);
    self.custom_cert.init(custom_cert);
    let config = client_config().await?;
    self.clinet_config.init(Arc::new(config));
    Ok(())
  }
}
pub fn read_certs_from_file(path: &str) -> Result<Vec<rustls::Certificate>> {
  let mut cert_chain_reader = BufReader::new(std::fs::File::open(path)?);
  let certs = rustls_pemfile::certs(&mut cert_chain_reader)?
    .into_iter()
    .map(rustls::Certificate)
    .collect::<Vec<_>>();
  assert!(!certs.is_empty());

  Ok(certs)
}

pub async fn client_config() -> Result<rustls::ClientConfig> {
  if !*TLS.skip_verify {
    let cert_store = tokio::task::spawn_blocking(|| {
      fn inner() -> Result<RootCertStore> {
        let mut cert_store = rustls::RootCertStore::empty();
        if let Some(path) = TLS.custom_cert.as_deref() {
          let certs = read_certs_from_file(path)?;
          for cert in certs {
            cert_store.add(&cert)?;
          }
        };
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
    rustls::ClientConfig::builder()
      .with_safe_defaults()
      .with_root_certificates(cert_store)
      .with_no_client_auth()
      .ok()
  } else {
    struct CustomVerifier {}

    impl ServerCertVerifier for CustomVerifier {
      fn verify_server_cert(
        &self,
        _: &rustls::Certificate,
        _: &[rustls::Certificate],
        _: &rustls::ServerName,
        _: &mut dyn Iterator<Item = &[u8]>,
        _: &[u8],
        _: std::time::SystemTime,
      ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
      }
    }
    rustls::ClientConfig::builder()
      .with_safe_defaults()
      .with_custom_certificate_verifier(Arc::new(CustomVerifier {}))
      .with_no_client_auth()
      .ok()
  }
}
