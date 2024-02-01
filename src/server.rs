use std::sync::{
  atomic::{AtomicI64, Ordering},
  Arc,
};

use arcstr::ArcStr;
use async_recursion::async_recursion;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use futures_util::{future::BoxFuture, StreamExt};
use lateinit::LateInit;
use tokio::{sync::oneshot, task::JoinHandle};
use tracing::instrument;
use uuid::Uuid;

use crate::OkExt;

pub trait PacketHandler =
  Fn(Packet) -> BoxFuture<'static, Result<ControlFlow<Packet>>> + Send + Sync + 'static;

use crate::{cipher::CIPHER, data::Packet, ControlFlow, NAMESPACE_MSGIST};

#[derive(Singleton, Default)]
pub struct Server {
  pub conns: DashMap<ArcStr, nats::Client>,
  pub remote_address: LateInit<Arc<DashMap<ArcStr, ArcStr>>>,
  pub packet_handler: LateInit<Box<dyn PacketHandler>>,
  pub inbox: DashMap<Arc<Uuid>, oneshot::Sender<Packet>>,
  pub room_map: DashMap<ArcStr, Arc<uuid::Uuid>>,
  pub subs: DashMap<ArcStr, DashMap<Arc<Uuid>, (AtomicI64, JoinHandle<()>)>>,
}
impl Server {
  pub async fn init(&self, remote_address: Arc<DashMap<ArcStr, ArcStr>>) -> Result<()> {
    remote_address.insert("mesagisto".into(), "mesagisto.itsusinn.site".into());
    for remote in remote_address.iter() {
      let client = nats::connect(remote.value().as_str()).await?;
      // TODO(logging)
      self.conns.insert(remote.key().to_owned(), client);
    }
    self.remote_address.init(remote_address);

    Ok(())
  }

  pub fn room_id(&self, room_address: ArcStr) -> Arc<Uuid> {
    let entry = self.room_map.entry(room_address.clone());
    entry
      .or_insert_with(|| {
        let unique_address = format!("{}{}", room_address, *CIPHER.origin_key);
        Arc::new(Uuid::new_v5(&NAMESPACE_MSGIST, unique_address.as_bytes()))
      })
      .clone()
  }

  #[async_recursion]
  pub async fn send(&self, pkt: Packet, server_name: &ArcStr) -> Result<()> {
    if let Some(remote) = self.conns.get(server_name) {
      remote
        .publish(pkt.room_id.as_hyphenated().to_string(), pkt.content.into())
        .await?;
    } else {
      // TODO(logging)
    };
    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn sub(&self, room_id: Arc<Uuid>, server_name: &ArcStr) -> Result<()> {
    if let Some(remote) = self.conns.get(server_name) {
      let entry = self
        .subs
        .entry(server_name.to_owned())
        .or_insert_with(Default::default);
      let client = remote.value().clone();
      let subs = entry
        .value()
        .entry(room_id.to_owned())
        .or_insert_with(move || {
          let handle = tokio::spawn(async move {
            let mut sub = client
              .subscribe(room_id.as_hyphenated().to_string())
              .await
              .expect("Failed to subscribe");
            while let Some(next) = sub.next().await {
              next.payload;
              todo!()
            }
          });
          (AtomicI64::new(0), handle)
        });
      let counter = &subs.value().0;

      counter.fetch_add(1, Ordering::SeqCst);
    } else {
      // TODO(logging)
    };

    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn unsub(&self, room_id: Arc<Uuid>, server: &ArcStr) -> Result<()> {
    let entry = self
      .subs
      .entry(server.to_owned())
      .or_insert_with(Default::default);

    if let Some(subs) = entry.value().get(&room_id) {
      subs.0.fetch_sub(1, Ordering::SeqCst);
      if subs.0.load(Ordering::SeqCst) < 1 {
        if let Some((_, former)) = entry.value().remove(&room_id) {

          former.1.abort();
        }
      }
    }
    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn request(&self, pkt: Packet, server_name: &ArcStr) -> Result<Packet> {
    if let Some(remote) = self.conns.get(server_name) {
      let msg = remote
        .request(pkt.room_id.as_hyphenated().to_string(), pkt.content.into())
        .await?;
      Packet {
        content: msg.payload.into(),
        room_id: pkt.room_id,
      }
      .ok()
    } else {
      Err(color_eyre::eyre::eyre!("No specified server found"))
    }
  }

  pub async fn respond(
    &self,
    pkt: Packet,
    inbox: nats::Message,
    server_name: &ArcStr,
  ) -> Result<()> {
    if let Some(remote) = self.conns.get(server_name)
      && let Some(reply) = inbox.reply
    {
      remote
        .publish(
          reply,
          pkt.content.into(),
        )
        .await?;
    } else {
      // TODO(logging) Err(color_eyre::eyre::eyre!("No specified server found"))
    }
    Ok(())
  }
}
