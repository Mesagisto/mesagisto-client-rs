use std::{collections::HashMap, fmt::Debug, sync::Arc};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use futures::future::BoxFuture;
use lateinit::LateInit;
use tokio::sync::oneshot;
use tracing::instrument;
use uuid::Uuid;

use crate::ResultExt;

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
  pub packet_handler: LateInit<Box<dyn PacketHandler>>,
  pub inbox: DashMap<Arc<Uuid>, oneshot::Sender<Packet>>,
  pub room_map: DashMap<ArcStr, Arc<uuid::Uuid>>,
}
impl Server {
  pub async fn init(&self, local: &str, remote: HashMap<ArcStr, ArcStr>) -> Result<()> {
    crate::quic::init(self, local, remote).await?;
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

  #[instrument(skip(self, content))]
  pub async fn send(&self, content: Packet, server: impl Into<ArcStr> + Debug) -> Result<()> {
    let payload = content.to_cbor()?;
    // FIXME TIMEOUT When server down
    if let Some(remote) = self.remote_endpoints.get(&server.into()) {
      let remote = remote.clone();
      let mut uni = remote.open_uni().await?;
      uni.write(&payload).await?;
      uni.finish().await?;
    } else {
      warn!("wtf")
    };

    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn sub(&self, room: Arc<Uuid>, server: impl Into<ArcStr> + Debug) -> Result<()> {
    let pkt = Packet::new_sub(room);
    self.send(pkt, server).await?;
    Ok(())
  }

  #[instrument(skip(self))]
  pub async fn unsub(&self, room: Arc<Uuid>, server: impl Into<ArcStr> + Debug) -> Result<()> {
    let pkt = Packet::new_unsub(room);
    self.send(pkt, server).await?;
    Ok(())
  }

  #[must_use]
  pub fn request(&self, mut content: Packet, server: ArcStr) -> oneshot::Receiver<Packet> {
    if content.inbox.is_none() {
      let inbox = box Inbox::default();
      content.inbox = Some(inbox);
    }
    let (sender, receiver) = oneshot::channel();
    let id = content.inbox.as_ref().unwrap().id();
    self.inbox.insert(id, sender);
    tokio::spawn(async move {
      SERVER.send(content, server).await.log();
    });
    receiver
  }
  pub async fn respond(&self,mut content: Packet, inbox: Arc<Uuid>, server: ArcStr ) -> Result<()> {
    content.inbox.replace(box Inbox::Respond { id: inbox });
    self.send(content, server).await?;
    Ok(())
  }
}
