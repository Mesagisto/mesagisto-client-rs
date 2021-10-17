use std::ops::Deref;

use crate::LateInit;
use aes_gcm::{aead::generic_array::GenericArray, aes::Aes256, AesGcm};
use thiserror::Error;
use typenum::U12;

#[derive(Error, Debug)]
pub enum CipherError {
  #[error(transparent)]
  EncryptError(#[from] aes_gcm::aead::Error),
}
type Key = GenericArray<u8, <sha2::Sha256 as sha2::Digest>::OutputSize>;
#[derive(Singleton, Default)]
pub struct Cipher {
  pub inner: LateInit<AesGcm<Aes256, U12>>,
  pub key: LateInit<Key>,
  pub origin_key: LateInit<String>,
}

impl Deref for Cipher {
  type Target = AesGcm<Aes256, U12>;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl Cipher {
  pub fn init(&self, key: &String) {
    use sha2::{Digest, Sha256};
    self.origin_key.init(key.to_owned());
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let hash_key = hasher.finalize();
    self.key.init(hash_key);
    use aes_gcm::aead::NewAead;
    use aes_gcm::Aes256Gcm;
    let key = aes_gcm::Key::from_slice(self.key.as_slice());
    let cipher = Aes256Gcm::new(key);
    self.inner.init(cipher);
  }
  pub fn new_nonce(&self) -> [u8; 12] {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);
    nonce
  }
}
