use std::convert::TryInto;

use arcstr::ArcStr;
use serde::{Deserialize, Serialize};

use crate::{OptionExt, ResultExt};

#[derive(Serialize, Deserialize, Debug)]
pub struct Profile {
  #[serde(with = "serde_bytes")]
  pub id: Vec<u8>,
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
impl Message {
  pub fn new(profile: Profile, id: i32, chain: Vec<MessageType>) -> Self {
    Message {
      profile,
      id: id.to_be_bytes().to_vec(),
      reply: None,
      chain,
    }
  }

  pub fn id_i64(&self) -> Option<i64> {
    i64::from_be_bytes(self.id.clone().try_into().ignore()?).some()
  }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum MessageType {
  Text {
    content: String,
  },
  Edit {
    content: String,
  },
  Image {
    #[serde(with = "serde_bytes")]
    id: Vec<u8>,
    url: Option<ArcStr>,
  },
}

#[cfg(test)]
mod test {
  use crate::data::message::{Message, MessageType, Profile};
  #[test]
  fn test() {
    let message = Message {
      profile: Profile {
        id: 232323i32.to_be_bytes().to_vec(),
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
          id: Vec::from("id"),
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
