use arcstr::ArcStr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Profile {
  pub id: i64,
  pub username: Option<String>,
  pub nick: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Message {
  pub profile: Profile,
  #[serde(with = "serde_bytes")]
  pub id: Vec<u8>,
  #[serde(with = "serde_bytes")]
  pub reply: Option<Vec<u8>>,
  pub chain: Vec<MessageType>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "t", content = "c")]
pub enum MessageType {
  Text { content: String },
  Image { id: ArcStr, url: Option<ArcStr> },
}

#[cfg(test)]
mod test {
  use crate::data::message::{Message, MessageType, Profile};
  use arcstr::ArcStr;
  #[test]
  fn test() {
    let message = Message {
      profile: Profile {
        id: 232323,
        username: None,
        nick: None,
      },
      id: Vec::from("id"),
      chain: vec![
        MessageType::Text {
          content: "this is text".to_string(),
        },
        MessageType::Text {
          content: "this is text".to_string(),
        },
        MessageType::Image {
          id: ArcStr::from("23232"),
          url: None,
        },
      ],
      reply: None,
    };
    let strw = serde_cbor::to_vec(&message).unwrap();
    println!("{} \n check in http://cbor.me/", hex::encode(&strw));
    let a = serde_cbor::from_slice::<Message>(&strw).is_ok();
    assert!(a);
  }
}
