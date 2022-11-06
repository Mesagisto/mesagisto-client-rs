pub mod events;
pub mod message;

use std::sync::Arc;

use aes_gcm::aead::Aead;
use color_eyre::eyre::Result;
use either::Either;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use self::{events::Event, message::Message};
use crate::{cipher::CIPHER, EitherExt, OkExt};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Packet {
  // [event/message]
  #[serde(rename = "t")]
  pub ty: String,
  #[serde(with = "serde_bytes", rename = "c")]
  pub content: Vec<u8>,
  #[serde(with = "serde_bytes", rename = "n")]
  pub nonce: Vec<u8>,
  #[serde(rename = "rid")]
  pub room_id: Arc<Uuid>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  pub inbox: Option<Box<Inbox>>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  pub ctl: Option<Ctl>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "t")]
pub enum Inbox {
  #[serde(rename = "req")]
  Request { id: Arc<Uuid> },
  #[serde(rename = "res")]
  Respond { id: Arc<Uuid> },
}
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "t")]
pub enum Ctl {
  #[serde(rename = "sub")]
  Sub,
  #[serde(rename = "unsub")]
  Unsub,
}
impl Default for Inbox {
  fn default() -> Self {
    Inbox::Request {
      id: Arc::new(Uuid::new_v4()),
    }
  }
}
impl Inbox {
  pub fn id(&self) -> Arc<Uuid> {
    match self {
      Inbox::Request { id } => id.to_owned(),
      Inbox::Respond { id } => id.to_owned(),
    }
  }
}
impl Packet {
  pub fn new_sub(room: Arc<Uuid>) -> Self {
    Self {
      ty: "ctl".to_string(),
      content: vec![],
      nonce: vec![],
      room_id: room,
      inbox: None,
      ctl: Some(Ctl::Sub),
    }
  }

  pub fn new_unsub(room: Arc<Uuid>) -> Self {
    Self {
      ty: "ctl".to_string(),
      content: vec![],
      nonce: vec![],
      room_id: room,
      inbox: None,
      ctl: Some(Ctl::Unsub),
    }
  }

  pub fn new(room: Arc<Uuid>, data: Either<message::Message, events::Event>) -> Result<Self> {
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
      ty: ty.into(),
      content: ciphertext,
      nonce: bytes_nonce.into(),
      room_id: room,
      inbox: None,
      ctl: None,
    }
    .ok()
  }

  pub fn from_cbor(data: &[u8]) -> Result<Packet> {
    let packet: Packet = ciborium::de::from_reader(data)?;
    Ok(packet)
  }

  pub fn decrypt(&self) -> Result<Either<message::Message, Event>> {
    let nonce = aes_gcm::Nonce::from_slice(&self.nonce);
    let plaintext = CIPHER.decrypt(nonce, self.content.as_ref())?;
    match self.ty.as_str() {
      "message" => ciborium::de::from_reader::<Message, &[u8]>(&*plaintext)?
        .to_left()
        .ok(),
      "event" => ciborium::de::from_reader::<Event, &[u8]>(&plaintext)?
        .to_right()
        .ok(),
      &_ => unreachable!(),
    }
  }

  pub fn to_cbor(&self) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    ciborium::ser::into_writer(&self, &mut data)?;
    Ok(data)
  }
}

#[cfg(test)]
mod test {
  use std::sync::Arc;

  use uuid::Uuid;

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
    CIPHER.init(&"this is key".to_string().into()).unwrap();
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
      from: 12113i64.to_be_bytes().to_vec()
    };
    let packet = Packet::new(Arc::new(Uuid::from_u128(0)), message.to_left()).unwrap();

    println!("{}", hex::encode(&packet.to_cbor().unwrap()));
    let packet2 = Packet::from_cbor(&packet.to_cbor().unwrap());
    assert!(packet2.is_ok());
  }
}
