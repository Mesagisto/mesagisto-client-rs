use arcstr::ArcStr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum Event {
  RequestImage {
    #[serde(with = "serde_bytes")]
    id: Vec<u8>,
  },
  RespondImage {
    #[serde(with = "serde_bytes")]
    id: Vec<u8>,
    url: ArcStr,
  },
  RequestEcho {
    // should contains group_id, group_name
    name: ArcStr,
  },
  RespondEcho {
    // should contains group_id, group_name
    name: ArcStr,
  },
}

#[cfg(test)]
mod test {
  use crate::data::events::*;
  #[test]
  fn test() {
    let event = Event::RequestImage {
      id: "dd".as_bytes().to_owned(),
    };
    let strw = serde_cbor::to_vec(&event).unwrap();
    println!("{} \n check in http://cbor.me/", hex::encode(&strw));
    let a = serde_cbor::from_slice::<Event>(&strw).is_ok();
    assert!(a);
  }
}
