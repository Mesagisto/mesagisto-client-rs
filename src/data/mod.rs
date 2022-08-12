pub mod events;
pub mod message;

use std::convert::TryFrom;

use aes_gcm::aead::Aead;
use color_eyre::{eyre, eyre::Result};
use either::Either;
use serde::{Deserialize, Serialize};

use self::{events::Event, message::Message};
use crate::{cipher::CIPHER, EitherExt, OkExt};

#[derive(Serialize, Deserialize)]
pub struct Packet {
  // [event/message]
  pub r#type: String,
  #[serde(with = "serde_bytes")]
  pub content: Vec<u8>,
  #[serde(with = "serde_bytes")]
  pub encrypt: Vec<u8>,
  pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct EncryptInfo {
  // [ase-256-gcm]
  cipher: String,
  //[u8;12]
  #[serde(with = "serde_bytes")]
  nonce: Vec<u8>,
}

impl Packet {
  pub fn from(data: Either<message::Message, events::Event>) -> Result<Self> {
    Self::encrypt_from(data)
  }

  fn encrypt_from(data: Either<message::Message, events::Event>) -> Result<Self> {
    let bytes_nonce = CIPHER.new_nonce();
    let nonce = aes_gcm::Nonce::from_slice(&bytes_nonce);

    let ty;
    let bytes = match data {
      Either::Left(m) => {
        ty = "message";
        let mut data = Vec::new();
        ciborium::ser::into_writer(&m, &mut data)?;
        data
      }
      Either::Right(e) => {
        ty = "event";
        let mut data = Vec::new();
        ciborium::ser::into_writer(&e, &mut data)?;
        data
      }
    };
    let ciphertext = CIPHER.encrypt(nonce, bytes.as_ref())?;
    Self {
      r#type: ty.into(),
      content: ciphertext,
      encrypt: bytes_nonce.into(),
      version: "v1".into(),
    }
    .ok()
  }

  pub fn from_cbor(data: &[u8]) -> Result<Either<message::Message, Event>> {
    let packet: Packet = ciborium::de::from_reader(data)?;
    let nonce = aes_gcm::Nonce::from_slice(&packet.encrypt);
    let plaintext = CIPHER.decrypt(nonce, packet.content.as_ref())?;
    match packet.r#type.as_str() {
  pub fn decrypt(&self) -> Result<Either<message::Message, Event>> {
      "message" => ciborium::de::from_reader::<Message, &[u8]>(&*plaintext)?
        .to_left()
        .ok(),
      "event" => ciborium::de::from_reader::<Event, &[u8]>(&plaintext)?
      &_ => unreachable!(),
    }
  }

  pub fn to_cbor(self) -> Result<Vec<u8>> {
    Ok(serde_cbor::to_vec(&self)?)
  }
}
impl TryFrom<Either<message::Message, events::Event>> for Packet {
  type Error = eyre::Error;

  fn try_from(value: Either<message::Message, events::Event>) -> Result<Self> {
    Self::encrypt_from(value)
  }
}
#[cfg(test)]
mod test {
  use crate::{
    cipher::CIPHER,
    data::{
      message::{self, Message},
      Packet,
    },
    EitherExt,
  };
  #[test]
  fn test() {
    CIPHER.init(&"this is key".to_string().into());
    let message = Message {
      profile: message::Profile {
        id: 1223232i64.to_be_bytes().to_vec(),
        username: None,
        nick: None,
      },
      id: Vec::from("id"),
      reply: None,
      chain: vec![
        message::MessageType::Text {
          content: "this is text".to_string(),
        },
        message::MessageType::Text {
          content: "this is text".to_string(),
        },
      ],
    };
    let packet = Packet::encrypt_from(message.to_left()).unwrap();

    println!("{}", hex::encode(&cbor_packet));
    let packet2 = Packet::from_cbor(&cbor_packet);
    assert!(packet2.is_ok());
  }
}
