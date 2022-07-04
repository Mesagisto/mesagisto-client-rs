use crate::cipher::CIPHER;
use crate::data::events::Event;
use crate::data::Packet;
use crate::EitherExt;
use anyhow::Ok;
use arcstr::ArcStr;
use dashmap::DashMap;
use lateinit::LateInit;
use nats::Connection;
use nats::header::HeaderMap;
use std::collections::HashSet;
use std::{future::Future, collections::HashMap};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace};

#[derive(Singleton, Default)]
pub struct Server {
  pub nc: LateInit<Connection>,
  pub address: LateInit<ArcStr>,
  pub cid: LateInit<u64>,
  pub lib_header: LateInit<HeaderMap>,
  pub endpoint: DashMap<ArcStr, JoinHandle<()>>,
  pub unique_address: DashMap<ArcStr, ArcStr>,
}
impl Server {
  pub async fn init(&self, address: &ArcStr) {
    self.address.init(address.to_owned());
    let nc = {
      let opts = nats::Options::new();
      info!("Connecting to nats server");
      let nc = opts
        .connect(self.address.as_str())
        .await
        .expect("Failed to connect nats server");
      info!("Connected sucessfully");
      nc
    };
    self.nc.init(nc);
    self.cid.init(self.nc.client_id().await);

    let header = {
      let mut inner = HashMap::default();
      let entry = inner.entry("meta".to_string()).or_insert_with(HashSet::default);
      entry.insert(format!("cid={}", *self.cid));
      entry.insert("lib".to_string());
      HeaderMap { inner }
    };
    self.lib_header.init(header);
  }

  pub fn unique_address(&self, address: &ArcStr) -> ArcStr {
    use sha2::{Digest, Sha256};
    let entry = self.unique_address.entry(address.clone());
    entry
      .or_insert_with(|| {
        let mut hasher = Sha256::new();
        let unique_address: ArcStr = format!("{}{}", address, *CIPHER.origin_key).into();
        hasher.update(unique_address);
        let hash_key = hasher.finalize();
        base64_url::encode(&hash_key).into()
      })
      .clone()
  }
  pub async fn send(
    &self,
    target: &ArcStr,
    address: &ArcStr,
    content: Packet,
    headers: Option<Arc<HeaderMap>>,
  ) -> anyhow::Result<()> {
    let unique_address = self.unique_address(address);
    let content = content.to_cbor()?;
    let headers = match headers {
      Some(headers) => headers,
      None => {

        let mut inner = HashMap::default();
        let entry = inner.entry("meta".to_string()).or_insert_with(HashSet::default);
        entry.insert(format!("sender={}", target));
        let headers = HeaderMap { inner };
        Arc::new(headers)
      }
    };
    self
      .nc
      .publish_with_reply_or_headers(unique_address.as_str(), None, Some(&*headers), content)
      .await?;
    Ok(())
  }

  pub async fn recv<H, Fut>(
    &self,
    target: ArcStr,
    address: &ArcStr,
    handler: H,
  ) -> anyhow::Result<()>
  where
    H: Fn(nats::Message, ArcStr) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
  {
    let address = self.unique_address(address);
    if self.endpoint.contains_key(&target) {
      return Ok(());
    }
    debug!("Creating sub on {} for {}", address, target);

    let sub = self.nc.subscribe(address.as_str()).await?;
    let clone_target = target.clone();
    // the task spawned below should use singleton,because it's "outside" of our logic
    let join = tokio::spawn(async move {
      async fn handle_incoming<H, Fut>(
        sub: &nats::Subscription,
        target: &ArcStr,
        handler: &H,
      ) -> Option<()>
      where
        H: Fn(nats::Message, ArcStr) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
      {
        let next: Option<nats::Message> = {
          let next = sub.next().await?;
          let meta = next.headers.as_ref()?;
          if !meta.is_not_self(target) {
            None
          } else if meta.is_remote_lib(*SERVER.cid) {
            async fn handle_lib_message(next: nats::Message) -> anyhow::Result<()> {
              debug!("Handling message sent by lib");
              let packet = Packet::from_cbor(&next.data)?;
              if packet.is_left() {
                return Ok(());
              }
              // Maybe, one day rustc could be clever enough to conclude that packet is right(Event)
              match packet.expect_right("Unreachable") {
                Event::RequestImage { id } => {
                  use crate::res::RES;
                  let url = match RES.get_photo_url(&id).await {
                    Some(s) => s,
                    None => {
                      info!("No image in db");
                      return Ok(());
                    }
                  };
                  let event: Event = Event::RespondImage { id, url };
                  let packet = Packet::from(event.to_right())?.to_cbor()?;
                  next.respond(packet).await.unwrap();
                  Ok(())
                }
                _ => Ok(()),
              }
            }
            if let Err(e) = handle_lib_message(next).await {
              error!(
                "Err when invoking nats message lib handler, {} \n backtrace {}",
                e,
                e.backtrace()
              );
            };
            None
          } else {
            Some(next)
          }
        };
        if let Some(next) = next {
          trace!("Received message of target {}", &target);
          if let Err(e) = handler(next, target.clone()).await {
            error!(
              "Err when invoking nats message handler, {} \n backtrace {}",
              e,
              e.backtrace()
            );
          }
        };
        None
      }
      loop {
        handle_incoming(&sub, &target, &handler).await;
      }
    });
    self.endpoint.insert(clone_target, join);
    Ok(())
  }

  pub async fn request(
    &self,
    address: &ArcStr,
    content: Packet,
    headers: Option<&HeaderMap>,
  ) -> anyhow::Result<nats::Message> {
    let address = self.unique_address(address);
    trace!("Requesting on {}", address);
    let inbox = self.nc.new_inbox();
    let sub = self.nc.subscribe(&inbox).await?;
    self
      .nc
      .publish_with_reply_or_headers(address.as_str(), Some(&inbox), headers, content.to_cbor()?)
      .await?;
    let reply = sub
      .next()
      .await
      .expect("the subscription has been unsubscribed or the connection is closed.");
    sub.unsubscribe().await?;
    Ok(reply)
  }
  pub fn unsub(&self, target: &ArcStr) {
    if let Some((_, join)) = self.endpoint.remove(target) {
      join.abort();
    }
  }
}

pub trait HeaderMapExt {
  fn is_not_self(&self, target: &ArcStr) -> bool;
  fn is_remote_lib(&self, cid: u64) -> bool;
}

impl HeaderMapExt for HeaderMap {
  #[inline]
  fn is_not_self(&self, target: &ArcStr) -> bool {
    let inner: &HashMap<String,HashSet<String>> = &self.inner;
    let meta = inner.get("meta");
    let meta = match meta {
      None => return false,
      Some(v) => v
    };

    let mut contains = false;
    for m in meta {
      if m == &format!("sender={}", target) {
        contains = true;
        break;
      }
    }
    !contains
  }
  #[inline]
  fn is_remote_lib(&self, cid: u64) -> bool {
    let inner: &HashMap<String,HashSet<String>> = &self.inner;
    let meta = inner.get("meta");
    let meta = match meta {
      None => return true,
      Some(v) => v
    };

    let mut contains_lib = false;
    let mut contains_cid = false;
    for m in meta {
      if m == "lib" {
        contains_lib = true;
      }
      if m == &format!("cid={}", cid) {
        contains_cid = true;
      }
    }
    contains_lib && !contains_cid
  }
}
