pub mod events;
pub mod message;

use aes_gcm::aead::Aead;
use either::Either;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use crate::{cipher::CIPHER, OkExt};

use self::{events::Event, message::Message};

#[derive(Serialize, Deserialize)]
pub struct Packet {
  // [event/message]
  pub r#type: String,
  #[serde(with = "serde_bytes")]
  pub content: Vec<u8>,
  #[serde(with = "serde_bytes")]
  pub encrypt: Option<Vec<u8>>,
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
  pub fn from(data: Either<message::Message, events::Event>) -> anyhow::Result<Self> {
    if *CIPHER.enable {
      Self::encrypt_from(data)
    } else {
      Self::plain_from(data)
    }
  }
  fn plain_from(data: Either<message::Message, events::Event>) -> anyhow::Result<Self> {
    let ty;
    let bytes = match data {
      Either::Left(m) => {
        ty = "message";
        serde_cbor::to_vec(&m)?
      }
      Either::Right(e) => {
        ty = "event";
        serde_cbor::to_vec(&e)?
      }
    };
    Self {
      r#type: ty.into(),
      content: bytes,
      encrypt: None,
      version: "v1".into(),
    }
    .ok()
  }
  fn encrypt_from(data: Either<message::Message, events::Event>) -> anyhow::Result<Self> {
    let bytes_nonce = CIPHER.new_nonce();
    let nonce = aes_gcm::Nonce::from_slice(&bytes_nonce);

    let ty;
    let bytes = match data {
      Either::Left(m) => {
        ty = "message";
        serde_cbor::to_vec(&m)?
      }
      Either::Right(e) => {
        ty = "event";
        serde_cbor::to_vec(&e)?
      }
    };
    let ciphertext = CIPHER.encrypt(nonce, bytes.as_ref())?;
    Self {
      r#type: ty.into(),
      content: ciphertext,
      encrypt: Some(bytes_nonce.into()),
      version: "v1".into(),
    }
    .ok()
  }
  pub fn from_cbor(data: &Vec<u8>) -> anyhow::Result<Either<message::Message, Event>> {
    let packet: Packet = serde_cbor::from_slice(data)?;
    let handle_encrypt =
      |packet: Packet, ty: bool| -> anyhow::Result<Either<message::Message, Event>> {
        let encrypt = packet.encrypt.unwrap();

        let nonce = aes_gcm::Nonce::from_slice(&encrypt);
        let plaintext = CIPHER.decrypt(nonce, packet.content.as_ref())?;
        if ty {
          let message: Message = serde_cbor::from_slice(&plaintext)?;
          Ok(Either::Left(message))
        } else {
          let event: Event = serde_cbor::from_slice(&plaintext)?;
          Ok(Either::Right(event))
        }
      };
    if packet.r#type == "message" {
      if packet.encrypt.is_none() {
        if *CIPHER.enable && !*CIPHER.refuse_plain {
          Err(anyhow::anyhow!("Refue plain message"))
        } else {
          let message: Message = serde_cbor::from_slice(&packet.content)?;
          Ok(Either::Left(message))
        }
      } else {
        handle_encrypt(packet, true)
      }
    } else if packet.r#type == "event" {
      if packet.encrypt.is_none() {
        if *CIPHER.enable && !*CIPHER.refuse_plain {
          Err(anyhow::anyhow!("Refue plain message"))
        } else {
          let event: Event = serde_cbor::from_slice(&packet.content)?;
          Ok(Either::Right(event))
        }
      } else {
        handle_encrypt(packet, false)
      }
    } else {
      unreachable!()
    }
  }

  pub fn to_cbor(self) -> anyhow::Result<Vec<u8>> {
    Ok(serde_cbor::to_vec(&self)?)
  }
}
impl TryFrom<Either<message::Message, events::Event>> for Packet {
  type Error = anyhow::Error;
  fn try_from(value: Either<message::Message, events::Event>) -> anyhow::Result<Self> {
    Self::encrypt_from(value)
  }
}
#[cfg(test)]
mod test {
  use crate::EitherExt;
  use crate::{
    cipher::CIPHER,
    data::{
      message::{self, Message},
      Packet,
    },
  };
  #[test]
  fn test() {
    CIPHER.init(&"this is key".to_string().into(), &true);
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
    let cbor_packet = serde_cbor::to_vec(&packet).unwrap();
    println!("{}", hex::encode(&cbor_packet));
    let packet2 = Packet::from_cbor(&cbor_packet);
    assert!(packet2.is_ok());
  }
}
