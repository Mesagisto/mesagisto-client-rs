use crate::cipher::CIPHER;
use crate::data::events::Event;
use crate::data::Packet;
use crate::EitherExt;
use crate::LogResultExt;
use anyhow::Ok;
use arcstr::ArcStr;
use dashmap::DashMap;
use futures::StreamExt;
use lateinit::LateInit;
use nats::header::HeaderMap;
use nats::{Client, HeaderValue};
use rand::prelude::random;
use std::future::Future;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace};

#[derive(Singleton, Default)]
pub struct Server {
  pub client: LateInit<Client>,
  pub address: LateInit<ArcStr>,
  pub cid: LateInit<u64>,
  pub lib_header: LateInit<HeaderMap>,
  pub endpoint: DashMap<ArcStr, JoinHandle<()>>,
  pub unique_address: DashMap<ArcStr, ArcStr>,
}
impl Server {
  pub async fn init(&self, address: &ArcStr) -> anyhow::Result<()> {
    self.address.init(address.to_owned());
    let client = {
      info!("Connecting to nats server");
      let nc = nats::connect(address.to_string()).await?;
      info!("Connected sucessfully");
      nc
    };
    self.client.init(client);
    // FIXME find a another thing that can replace client id
    let cid: u16 = random();
    self.cid.init(cid as u64);

    let header = {
      let mut header = HeaderMap::new();
      header.append(
        "meta",
        HeaderValue::from_str(&format!("cid={}", *self.cid))?,
      );
      header.append("meta", HeaderValue::from_static("lib"));
      header
    };
    self.lib_header.init(header);
    Ok(())
  }
  pub fn new_lib_header(&self) -> anyhow::Result<HeaderMap> {
    let mut header = HeaderMap::new();
    header.append(
      "meta",
      HeaderValue::from_str(&format!("cid={}", *self.cid))?,
    );
    header.append("meta", HeaderValue::from_static("lib"));
    Ok(header)
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
    headers: Option<HeaderMap>,
  ) -> anyhow::Result<()> {
    let unique_address = self.unique_address(address);
    let payload = content.to_cbor()?;
    let headers = match headers {
      Some(headers) => headers,
      None => {
        let mut header = HeaderMap::new();
        header.append(
          "meta",
          HeaderValue::from_str(&format!("sender={}", target))?,
        );
        header
      }
    };

    self
      .client
      .publish_with_headers(
        unique_address.to_string(),
        headers,
        bytes::Bytes::from(payload),
      )
      .await
      .map_err(|e| anyhow::anyhow!(e))?;
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

    let mut sub = self.client.subscribe(address.to_string()).await?;
    let clone_target = target.clone();
    // the task spawned below should use singleton,because it's "outside" of our logic
    let join = tokio::spawn(async move {
      while let Some(next) = sub.next().await {
        handle_incoming(next, &target, &handler)
          .await
          .log_if_error("Err when handing incoming nats message");
      }
      async fn handle_incoming<H, Fut>(
        next: nats::Message,
        target: &ArcStr,
        handler: &H,
      ) -> anyhow::Result<()>
      where
        H: Fn(nats::Message, ArcStr) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
      {
        let next: Option<nats::Message> = if let Some(meta) = next.headers.as_ref() {
          if !meta.is_not_self(target) {
            None
          } else if meta.is_remote_lib(*SERVER.cid) {
            Server::handle_lib_message(next).await?;
            None
          } else {
            Some(next)
          }
        } else {
          None
        };
        if let Some(next) = next {
          trace!("Received message of target {}", &target);
          handler(next, target.clone()).await?;
        };
        Ok(())
      }
    });
    self.endpoint.insert(clone_target, join);
    Ok(())
  }

  pub async fn request(
    &self,
    address: &ArcStr,
    content: Packet,
    headers: HeaderMap,
  ) -> anyhow::Result<nats::Message> {
    let address = self.unique_address(address);
    trace!("Requesting on {}", address);
    let inbox = self.client.new_inbox();
    let mut sub = self.client.subscribe(inbox.clone()).await?;
    self
      .client
      .publish_with_reply_and_headers(
        address.to_string(),
        inbox,
        headers,
        bytes::Bytes::from(content.to_cbor()?),
      )
      .await
      .map_err(|e| anyhow::anyhow!(e))?;
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
  async fn handle_lib_message(next: nats::Message) -> anyhow::Result<()> {
    debug!("Handling message sent by lib");
    let packet = Packet::from_cbor(&next.payload)?;
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
        let payload = Packet::from(event.to_right())?.to_cbor()?;

        let reply = next
          .reply
          .ok_or_else(|| anyhow::anyhow!("No reply subject to reply to"))?;
        SERVER
          .client
          .publish(reply, bytes::Bytes::from(payload))
          .await
          .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
      }
      _ => Ok(()),
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
    let meta = self.get_all("meta").into_iter();

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
    let meta = self.get_all("meta").into_iter();
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
