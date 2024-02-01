use std::ops::Deref;

use aes::Aes256;
use aes_gcm_siv::{Aes256GcmSiv, AesGcmSiv, KeyInit};
use arcstr::ArcStr;
use color_eyre::eyre::Result;
use lateinit::LateInit;

#[derive(Singleton, Default)]
pub struct Cipher {
  inner: LateInit<AesGcmSiv<Aes256>>,
  pub key: LateInit<[u8; 32]>,
  pub nonce: LateInit<aes_gcm_siv::Nonce>,
  pub origin_key: LateInit<ArcStr>,
}

impl Deref for Cipher {
  type Target = AesGcmSiv<Aes256>;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl Cipher {
  pub fn init(&self, key: &ArcStr) -> Result<()> {
    use sha2::{Digest, Sha256};
    self.origin_key.init(key.to_owned());
    let hash_key = Sha256::digest(key).into();
    self.key.init(hash_key);
    let nonce = aes_gcm_siv::Nonce::from_slice(hash_key[0..12].try_into().unwrap()).to_owned();
    self.nonce.init(nonce);
    let cipher = Aes256GcmSiv::new_from_slice(self.key.as_slice())?;
    self.inner.init(cipher);
    Ok(())
  }

}
