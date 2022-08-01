use arcstr::ArcStr;
use dashmap::DashMap;

#[derive(Singleton, Default)]
pub struct Pool {
  inner: DashMap<ArcStr, nats::Client>,
}

impl Pool {}
