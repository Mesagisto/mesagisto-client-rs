use std::ops::Deref;

use aes_gcm::{aead::generic_array::GenericArray, aes::Aes256, AesGcm};
use arcstr::ArcStr;
use lateinit::LateInit;
use typenum::{UInt, UTerm, B0, B1, U12};

type Key = GenericArray<u8, UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>>;
#[derive(Singleton, Default)]
pub struct Cipher {
  inner: LateInit<AesGcm<Aes256, U12>>,
  pub key: LateInit<Key>,
  pub origin_key: LateInit<ArcStr>,
}

impl Deref for Cipher {
  type Target = AesGcm<Aes256, U12>;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl Cipher {
  pub fn init(&self, key: &ArcStr) {
    let hash_key = {
      use sha2::{Digest, Sha256};
      self.origin_key.init(key.to_owned());
      let mut hasher = Sha256::new();
      hasher.update(key.as_bytes());
      hasher.finalize()
    };
    self.key.init(hash_key);
    let cipher = {
      use aes_gcm::aead::NewAead;
      use aes_gcm::Aes256Gcm;
      let key = aes_gcm::Key::from_slice(self.key.as_slice());
      Aes256Gcm::new(key)
    };
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
