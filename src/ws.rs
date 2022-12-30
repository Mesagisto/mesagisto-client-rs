use std::{
  ops::{ControlFlow, Deref},
  time::Duration,
};

use arcstr::ArcStr;
use async_recursion::async_recursion;
use color_eyre::eyre::{eyre, Result};
use dashmap::mapref::entry::Entry;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, Connector, MaybeTlsStream, WebSocketStream};

use crate::tls::TLS;

pub type WsConn = tokio::sync::mpsc::Sender<Message>;

use crate::{data::Packet, info, server::SERVER, ResultExt};

pub async fn init() -> Result<()> {
  for entry in SERVER.remote_address.iter() {
    connect(entry.key()).await.log();
  }
  Ok(())
}
pub async fn connect(server_id: &ArcStr) -> Result<()> {
  if let Some((_, _former)) = SERVER.conns.remove(server_id) {
    // TODO former.close(quinn::VarInt::from_u32(2000), b"conflict");
  };
  if let Some(remote_endpoint) = SERVER.conns.try_entry(server_id.clone())
  && let Some(remote_address) = SERVER.remote_address.get(server_id) {
    connect_with_entry(server_id, &remote_address, remote_endpoint).await?;
    drop(remote_address);
    if let Some(subs) = SERVER.subs.get(server_id){
      let subs = subs.to_owned();
      for room_id in subs.into_iter() {
        tracing::debug!("Sub {} on {}",room_id, server_id);
        let pkt = Packet::new_sub(room_id);
        SERVER.send(pkt, server_id).await.log();
      }
    }
    Ok(())
  } else {
    Err(eyre!("Server not exists name {}", server_id))
  }
}

#[async_recursion]
pub async fn connect_with_entry(
  server_id: &'async_recursion ArcStr,
  remote_address: &'async_recursion ArcStr,
  remote_endpoint: Entry<'async_recursion, ArcStr, WsConn>,
) -> Result<()> {
  info!("log-connecting", address = remote_address.as_str());
  let remote_url = url::Url::parse(remote_address)?;

  let (conn, _) = tokio_tungstenite::connect_async_tls_with_config(
    remote_url,
    None,
    Some(Connector::Rustls(TLS.clinet_config.deref().to_owned())),
  )
  .await?;

  let (mut write, read) = conn.split();
  info!("log-connected");

  let server_id = server_id.to_owned();
  let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(128);
  {
    remote_endpoint.or_insert(tx);
  }
  tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
      tokio::select! {
        _ = interval.tick() => {
          if let None = write.send(
            Message::Ping(rand::random::<i64>().to_be_bytes().to_vec())
          ).await.log() {
            rx.close();
            while let Some(_) = rx.recv().await {
              tracing::warn!("lost message")
            }
            break;
          }
        }
        Some(msg) = rx.recv() => {
          if let None = write.send(msg).await.log() {
            rx.close();
            while let Some(_) = rx.recv().await {
              tracing::warn!("lost message")
            }
            break;
          }
        }
      }
    }
  });
  tokio::spawn(async move {
    receive_ws(read).await.log();
    tracing::debug!("WS disconnected");
    let mut retry_times = 1;
    loop {
      tracing::debug!("Trying to connect WS server {server_id}, retry times {retry_times}");
      match connect(&server_id).await.log() {
        Some(_) => break,
        None => {
          retry_times += 1;
          if retry_times >= 150 {
            tracing::warn!("Failed to reconnect WS server {server_id}");
            break;
          }
          tokio::time::sleep(Duration::from_secs(10)).await;
          tracing::warn!("Retrying to connect WS server {server_id}");
          continue;
        }
      }
    }
  });
  Ok(())
}

pub async fn receive_ws(
  mut read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
) -> Result<()> {
  // keep receiving until a error
  while let Some(Ok(recv)) = read.next().await {
    match recv {
      Message::Binary(data) => {
        let packet: Result<Packet, _> = ciborium::de::from_reader(&*data);
        if let Some(packet) = packet.log() {
          tokio::spawn(async move {
            let packet_handler = SERVER.packet_handler.deref();
            if let Some(ControlFlow::Break(pkt)) = packet_handler(packet).await.log() {
              SERVER.handle_rest_pkt(pkt).await;
            }
          });
        }
      }
      Message::Pong(_) => {}
      Message::Close(_) => {
        break;
      }
      _ => {}
    }
  }
  Ok(())
}
