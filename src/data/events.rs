use arcstr::ArcStr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
  pub data: EventType,
}
impl Event {
  pub fn new(kind: EventType) -> Self {
    Self { data: kind }
  }
}
impl From<EventType> for Event {
  fn from(ty: EventType) -> Self {
    Self { data: ty }
  }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "t", content = "c")]
pub enum EventType {
  RequestImage { id: ArcStr },
  RespondImage { id: ArcStr, url: ArcStr },
}
