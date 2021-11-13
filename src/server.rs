use crate::cipher::CIPHER;
use crate::data::events::{Event, EventType};
use crate::data::Packet;
use crate::{EitherExt, LateInit};
use arcstr::ArcStr;
use dashmap::DashMap;
use nats::{asynk::Connection, Headers};
use std::fmt::Debug;
use std::future::Future;

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
  #[error(transparent)]
  IOError(#[from] std::io::Error),
  #[error(transparent)]
  DataError(#[from] crate::data::DataError),
}

#[derive(Singleton, Default)]
pub struct Server {
  pub nc: LateInit<Connection>,
  pub address: LateInit<String>,
  pub cid: LateInit<String>,
  pub nats_header: LateInit<Headers>,
  pub lib_header: LateInit<Headers>,
  pub endpoint: DashMap<i64, bool>,
  pub compat_address: DashMap<ArcStr, ArcStr>,
}
impl Server {
  pub async fn init(&self, address: &String) {
    self.address.init(address.to_owned());
    let nc = {
      let opts = nats::asynk::Options::new();
      log::info!("Connecting to nats server");
      let nc = opts
        .with_name("telegram client")
        .connect(&self.address)
        .await
        .expect("Failed to connect nats server");
      log::info!("Connected sucessfully");
      nc
    };
    self.nc.init(nc);
    self.cid.init(self.nc.client_id().to_string());
    let header = {
      use std::collections::HashMap;
      use std::collections::HashSet;
      let mut inner = HashMap::default();
      let entry = inner
        .entry("meta".to_string())
        .or_insert_with(HashSet::default);
      entry.insert(format!("cid={}", *self.cid));
      Headers { inner }
    };
    self.nats_header.init(header);
    let header = {
      use std::collections::HashMap;
      use std::collections::HashSet;
      let mut inner = HashMap::default();
      let entry = inner
        .entry("meta".to_string())
        .or_insert_with(HashSet::default);
      entry.insert(format!("cid={}", *self.cid));
      entry.insert("lib".to_string());
      Headers { inner }
    };
    self.lib_header.init(header);
  }

  pub fn compat_address(&self, address: &ArcStr) -> ArcStr {
    use sha2::{Digest, Sha256};
    let entry = self.compat_address.entry(address.clone());
    entry
      .or_insert_with(|| {
        let mut hasher = Sha256::new();
        hasher.update(CIPHER.unique_address(address));
        let hash_key = hasher.finalize();
        format!("compat.{}", base64_url::encode(&hash_key)).into()
      })
      .clone()
  }
  //fixme: not a correct api
  pub async fn send_and_receive<H, Fut>(
    &self,
    target: i64,
    address: ArcStr,
    content: Packet,
    handler: H,
  ) -> Result<(), ServerError>
  where
    H: Fn(nats::asynk::Message, i64) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
  {
    let compat_address = self.compat_address(&address);
    let content = content.to_cbor()?;

    self
      .nc
      .publish_with_reply_or_headers(
        &compat_address.as_str(),
        None,
        Some(&*self.nats_header),
        content,
      )
      .await?;
    self
      .try_create_endpoint(target, compat_address, handler)
      .await?;
    Ok(())
  }

  pub async fn try_create_endpoint<H, Fut>(
    &self,
    target: i64,
    address: ArcStr,
    handler: H,
  ) -> Result<(), ServerError>
  where
    H: Fn(nats::asynk::Message, i64) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
  {
    log::debug!("Trying to create sub for {}", target);
    if self.endpoint.contains_key(&target) {
      return Ok(());
    }
    self.endpoint.insert(target.clone(), true);

    log::debug!(
      "Creating sub on {} for {} with compatibility",
      address,
      target
    );
    let sub = self.nc.subscribe(address.as_str()).await?;
    // the task spawned below should use singleton,because it's "outside" of our logic
    tokio::spawn(async move {
      async fn handle_incoming<H, Fut>(
        sub: &nats::asynk::Subscription,
        target: i64,
        handler: &H,
      ) -> Option<()>
      where
        H: Fn(nats::asynk::Message, i64) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
      {
        let next: Option<nats::asynk::Message> = {
          let next = sub.next().await?;
          let meta = next.headers.as_ref()?.get("meta")?;
          if meta.contains(&format!("cid={}", &*SERVER.cid)) {
            None
          } else {
            if meta.contains("lib") {
              async fn handle_lib_message(next: nats::asynk::Message) -> anyhow::Result<()> {
                log::debug!("Handling message sent by lib");
                let packet = Packet::from_cbor(&next.data)?;
                if packet.is_left() {
                  return Ok(());
                }
                // Maybe, one day rustc could be clever enough to conclude that packet is right(Event)
                match packet.expect_right("Unreachable").data {
                  EventType::RequestImage { id } => {
                    use crate::res::RES;
                    let url = match RES.get_photo_url(&id).await {
                      Some(s) => s,
                      None => {
                        log::info!("No image in db");
                        return Ok(());
                      }
                    };
                    let event: Event = EventType::RespondImage { id, url }.into();
                    let packet = Packet::from(event.to_right())?.to_cbor()?;
                    next.respond(packet).await.unwrap();
                    Ok(())
                  }
                  _ => Ok(()),
                }
              }
              if let Err(e) = handle_lib_message(next).await {
                log::error!(
                  "Err when invoking nats message lib handler, {} \n backtrace {}",
                  e,
                  e.backtrace()
                );
              };
              None
            } else {
              Some(next)
            }
          }
        };
        if let Some(next) = next {
          log::trace!("Received message of target {}", target);
          if let Err(e) = handler(next, target).await {
            log::error!(
              "Err when invoking nats message handler, {} \n backtrace {}",
              e,
              e.backtrace()
            );
          }
        };
        None
      }
      loop {
        handle_incoming(&sub, target, &handler).await;
      }
    });
    Ok(())
  }

  pub async fn request(
    &self,
    address: &ArcStr,
    content: Packet,
    headers: Option<&Headers>,
  ) -> Result<nats::asynk::Message, ServerError> {
    let address = self.compat_address(address);
    log::trace!("Requesting on {}", address);
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
}
