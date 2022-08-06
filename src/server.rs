use std::{future::Future, time::Duration};

use arcstr::ArcStr;
use color_eyre::eyre::{eyre, Result};
use dashmap::DashMap;
use futures::StreamExt;
use lateinit::LateInit;
use nats::{header::HeaderMap, Client, HeaderValue};
use rand::prelude::random;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, trace};

use crate::{
  cipher::CIPHER,
  data::{events::Event, Packet},
  EitherExt, LogResultExt,
};

#[derive(Singleton, Default, Debug)]
pub struct Server {
  pub client: LateInit<Client>,
  pub address: LateInit<ArcStr>,
  pub cid: LateInit<u64>,
  pub lib_header: LateInit<HeaderMap>,
  pub endpoint: DashMap<ArcStr, JoinHandle<()>>,
  pub unique_address: DashMap<ArcStr, ArcStr>,
}
impl Server {
  pub async fn init(&self, address: &ArcStr) -> Result<()> {
    self.address.init(address.to_owned());
    let client = {
      info!("{}", t!("log.connecting", address = address));
      let nc = nats::connect(address.to_string()).await?;
      info!("{}", t!("log.connected"));
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

  pub fn new_lib_header(&self) -> Result<HeaderMap> {
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

  #[instrument(skip(self, content))]
  pub async fn send(
    &self,
    target: &ArcStr,
    address: &ArcStr,
    content: Packet,
    headers: Option<HeaderMap>,
  ) -> Result<()> {
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
      .map_err(|e| eyre!(e))?;
    Ok(())
  }

  pub async fn recv<H, Fut>(&self, target: ArcStr, address: &ArcStr, handler: H) -> Result<()>
  where
    H: Fn(nats::Message, ArcStr) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
  {
    let address = self.unique_address(address);
    if self.endpoint.contains_key(&target) {
      return Ok(());
    }
    debug!(
      "{}",
      t!("log.create-sub", address = &address, target = &target)
    );

    let mut sub = self
      .client
      .subscribe(address.to_string())
      .await
      .map_err(|e| eyre!(e))?;
    let clone_target = target.clone();
    // the task spawned below should use singleton,because it's "outside" of our
    // logic
    let join = tokio::spawn(async move {
      while let Some(next) = sub.next().await {
        let res = tokio::time::timeout(
          Duration::from_secs_f32(13.0),
          handle_incoming(next, &target, &handler),
        )
        .await;
        match res {
          Ok(v) => {
            v.log_if_error(&t!("log.log-callback-err"));
          }
          Err(_) => {
            error!("{}", t!("log.log-callback-timeout"));
          }
        };
      }
      #[instrument(skip(next, handler))]
      async fn handle_incoming<H, Fut>(
        next: nats::Message,
        target: &ArcStr,
        handler: &H,
      ) -> Result<()>
      where
        H: Fn(nats::Message, ArcStr) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
      {
        let next: Option<nats::Message> = if let Some(meta) = next.headers.as_ref() {
          if !meta.is_not_self(target) {
            None
          } else if meta.is_remote_lib(*SERVER.cid) {
            Server::handle_lib_pkt(next).await?;
            None
          } else {
            Some(next)
          }
        } else {
          None
        };
        if let Some(next) = next {
          trace!("{}", t!("log.invoke-handler", target = &target));
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
  ) -> Result<nats::Message> {
    let address = self.unique_address(address);
    trace!("{}", t!("log.send-request"));
    let inbox = self.client.new_inbox();
    let mut sub = self
      .client
      .subscribe(inbox.clone())
      .await
      .map_err(|e| eyre!(e))?;
    self
      .client
      .publish_with_reply_and_headers(
        address.to_string(),
        inbox,
        headers,
        bytes::Bytes::from(content.to_cbor()?),
      )
      .await
      .map_err(|e| eyre!(e))?;
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

  async fn handle_lib_pkt(next: nats::Message) -> Result<()> {
    debug!("{}", t!("log.handle-lib-msg"));
    let packet = Packet::from_cbor(&next.payload)?;
    if packet.is_left() {
      return Ok(());
    }
    // Maybe, one day rustc could be clever enough to conclude that packet is
    // right(Event)
    match packet.expect_right("Unreachable") {
      Event::RequestImage { id } => {
        use crate::res::RES;
        let url = match RES.get_photo_url(&id).await {
          Some(s) => s,
          None => {
            info!("{}", t!("log.image-not-found"));
            return Ok(());
          }
        };
        let event: Event = Event::RespondImage { id, url };
        let payload = Packet::from(event.to_right())?.to_cbor()?;

        let reply = next
          .reply
          .ok_or_else(|| eyre!("No reply subject to reply to"))?;
        SERVER
          .client
          .publish(reply, bytes::Bytes::from(payload))
          .await
          .map_err(|e| eyre!(e))?;
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
