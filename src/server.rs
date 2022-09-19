use std::{collections::HashSet, sync::Arc, time::Duration};

use arcstr::ArcStr;
use async_recursion::async_recursion;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use futures::future::BoxFuture;
use lateinit::LateInit;
use tokio::{sync::oneshot, time::timeout};
use tracing::instrument;
use uuid::Uuid;

use crate::{quic, ResultExt};

pub trait PacketHandler =
  Fn(Packet) -> BoxFuture<'static, Result<ControlFlow<Packet>>> + Send + Sync + 'static;
use crate::{
  cipher::CIPHER,
  data::{Inbox, Packet},
  ControlFlow, NAMESPACE_MSGIST,
};

#[derive(Singleton, Default)]
pub struct Server {
  pub endpoint: LateInit<quinn::Endpoint>,
  pub remote_endpoints: DashMap<ArcStr, quinn::Connection>,
  pub remote_address: LateInit<Arc<DashMap<ArcStr, ArcStr>>>,
  pub packet_handler: LateInit<Box<dyn PacketHandler>>,
  pub inbox: DashMap<Arc<Uuid>, oneshot::Sender<Packet>>,
  pub room_map: DashMap<ArcStr, Arc<uuid::Uuid>>,
  pub subs: DashMap<ArcStr, HashSet<Arc<Uuid>>>,
}
impl Server {
  pub async fn init(
    &self,
    local: &str,
    remote_address: Arc<DashMap<ArcStr, ArcStr>>,
  ) -> Result<()> {
    self.remote_address.init(remote_address);
    crate::quic::init(self, local).await?;
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

  pub async fn handle_rest_pkt(&self, mut pkt: Packet) {
    match pkt.inbox.take() {
      Some(inbox) => match *inbox {
        Inbox::Request { .. } => {}
        Inbox::Respond { id } => {
          if let Some(sender) = self.inbox.remove(&id) {
            let _ = sender.1.send(pkt);
          }
        }
      },
      None => {}
    }
  }

  #[async_recursion]
  pub async fn send(&self, content: Packet, server_name: &ArcStr) -> Result<()> {
    let payload = content.to_cbor()?;
    let mut reconnect = true;
    if let Some(remote) = self.remote_endpoints.get(server_name) {
      let remote = remote.clone();
      if let Ok(uni) = timeout(Duration::from_secs(2), remote.open_uni()).await
      && let Some(mut uni) = match uni {
        Ok(v) => {
          reconnect = false;
          Some(v)
        },
        Err(quinn::ConnectionError::ApplicationClosed(e)) => {
          if e.error_code == quinn::VarInt::from_u32(2000) {
            reconnect = false;
          }
          None
        }
        Err(_) => {
          None
        },
      }
      && let Ok(_) = uni.write(&payload).await
      && let Ok(_) = uni.finish().await {
        reconnect = false;
      }
    } else {
      reconnect = true;
    };
    if reconnect {
      info!("reconnecting to {}", server_name);
      quic::connect(self, server_name).await?;
      self.send(content, server_name).await?;
    }
    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn sub(&self, room_id: Arc<Uuid>, server_name: &ArcStr) -> Result<()> {
    let mut entry = self
      .subs
      .entry(server_name.to_owned())
      .or_insert_with(Default::default);
    entry.insert(room_id.clone());
    entry.shrink_to_fit();
    drop(entry);
    let pkt = Packet::new_sub(room_id);
    self.send(pkt, server_name).await?;
    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn unsub(&self, room: Arc<Uuid>, server: &ArcStr) -> Result<()> {
    let mut entry = self
      .subs
      .entry(server.to_owned())
      .or_insert_with(Default::default);
    entry.remove(&room);
    entry.shrink_to_fit();
    drop(entry);
    let pkt = Packet::new_unsub(room);
    self.send(pkt, server).await?;
    Ok(())
  }

  #[must_use]
  pub fn request(&self, mut content: Packet, server_name: &ArcStr) -> oneshot::Receiver<Packet> {
    if content.inbox.is_none() {
      let inbox = box Inbox::default();
      content.inbox = Some(inbox);
    }
    let (sender, receiver) = oneshot::channel();
    let id = content.inbox.as_ref().unwrap().id();
    self.inbox.insert(id, sender);
    let server_name = server_name.to_owned();
    tokio::spawn(async move {
      SERVER.send(content, &server_name).await.log();
    });
    receiver
  }

  pub async fn respond(
    &self,
    mut content: Packet,
    inbox: Arc<Uuid>,
    server: &ArcStr,
  ) -> Result<()> {
    content.inbox.replace(box Inbox::Respond { id: inbox });
    self.send(content, server).await?;
    Ok(())
  }
}
