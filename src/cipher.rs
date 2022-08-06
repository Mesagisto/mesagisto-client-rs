use std::ops::Deref;

use aes_gcm::{
  aead::generic_array::GenericArray, aes::Aes256, Aes256Gcm, AesGcm, KeyInit, KeySizeUser,
};
use arcstr::ArcStr;
use color_eyre::eyre::Result;
use lateinit::LateInit;
use typenum::U12;

pub type Key<B> = GenericArray<u8, <B as KeySizeUser>::KeySize>;

#[derive(Singleton, Default)]
pub struct Cipher {
  inner: LateInit<AesGcm<Aes256, U12>>,
  pub key: LateInit<Key<Aes256Gcm>>,
  pub origin_key: LateInit<ArcStr>,
}

impl Deref for Cipher {
  type Target = AesGcm<Aes256, U12>;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl Cipher {
  pub fn init(&self, key: &ArcStr) -> Result<()> {
    let hash_key = {
      use sha2::{Digest, Sha256};
      self.origin_key.init(key.to_owned());
      let mut hasher = Sha256::new();
      hasher.update(key.as_bytes());
      hasher.finalize()
    };
    self.key.init(hash_key);
    let cipher = Aes256Gcm::new_from_slice(self.key.as_slice())?;
    self.inner.init(cipher);
    Ok(())
  }

  pub fn new_nonce(&self) -> [u8; 12] {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);
    nonce
  }
}
