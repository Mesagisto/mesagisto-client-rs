use std::{
  net::{SocketAddr, SocketAddrV4, SocketAddrV6},
  ops::{ControlFlow, Deref},
  time::Duration,
};

use arcstr::ArcStr;
use async_recursion::async_recursion;
use color_eyre::eyre::{eyre, Result};
use dashmap::mapref::entry::Entry;
use futures::StreamExt;
use trust_dns_resolver::{
  config::{ResolverConfig, ResolverOpts},
  TokioAsyncResolver,
};
use url::Host;

use crate::{data::Packet, server, server::SERVER, ResultExt};

pub async fn init(server: &server::Server, local: &str) -> Result<()> {
  let local_socket = local.parse::<SocketAddr>()?;
  let mut endpoint = quinn::Endpoint::client(local_socket)?;
  endpoint.set_default_client_config(crate::tls::client_config().await?);
  server.endpoint.init(endpoint);
  for entry in server.remote_address.iter() {
    connect(server, entry.key()).await.log();
  }
  Ok(())
}
#[async_recursion]
pub async fn connect_with_entry(
  server: &'async_recursion server::Server,
  server_id: &'async_recursion ArcStr,
  remote_address: &'async_recursion ArcStr,
  remote_endpoint: Entry<'async_recursion, ArcStr, quinn::Connection>,
) -> Result<()> {
  let endpoint = &*server.endpoint;
  info!("{}", t!("log.connecting", address = &remote_address));
  let remote_url = url::Url::parse(remote_address)?;

  if remote_url.scheme() != "msgist" {
    todo!("wrong scheme")
  }
  let mut server_name: &str = "";
  let resolver =
    TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default()).unwrap();
  let remote_socket: SocketAddr = match remote_url.host() {
    Some(Host::Ipv4(v)) => SocketAddrV4::new(v, remote_url.port().unwrap_or(6996)).into(),
    Some(Host::Ipv6(v)) => SocketAddrV6::new(v, remote_url.port().unwrap_or(6996), 0, 0).into(),
    Some(Host::Domain(v)) => {
      let response = resolver.lookup_ip(v).await?;
      let ip = response.iter().next().unwrap();
      server_name = v;
      SocketAddr::new(ip, remote_url.port().unwrap_or(6996))
    }
    None => todo!(),
  };
  if server_name.is_empty() {
    // TODO
    server_name = "localhost"
  }
  let new_conn = endpoint.connect(remote_socket, server_name)?.await?;
  info!("{}", t!("log.connected"));
  {
    remote_endpoint.or_insert(new_conn.connection);
  }
  let server_id = server_id.to_owned();
  tokio::spawn(async move {
    receive_uni_stream(new_conn.uni_streams).await.log();
    debug!("QUIC disconnected");
    let mut retry_times = 60;
    loop {
      match connect(&SERVER, &server_id).await.log() {
        Some(_) => break,
        None => {
          retry_times -= 1;
          if retry_times <= 0 {
            warn!("Failed to reconnect QUIC server {server_id}");
            break;
          }
          tokio::time::sleep(Duration::from_secs(1)).await;
          warn!("Retrying to connect QUIC server {server_id}");
          continue;
        }
      }
    }
  });
  Ok(())
}
pub async fn connect(server: &server::Server, server_id: &ArcStr) -> Result<()> {
  if let Some((_, former)) = server.remote_endpoints.remove(server_id) {
    former.close(quinn::VarInt::from_u32(2000), b"conflict");
  };
  if let Some(remote_endpoint) = server.remote_endpoints.try_entry(server_id.clone())
  && let Some(remote_address) = server.remote_address.get(server_id) {
    connect_with_entry(server, server_id, &remote_address, remote_endpoint).await?;
    drop(remote_address);
    if let Some(subs) = server.subs.get(server_id){
      let subs = subs.to_owned();
      for room_id in subs.into_iter() {
        debug!("Sub {} on {}",room_id, server_id);
        let pkt = Packet::new_sub(room_id);
        server.send(pkt, server_id).await.log();
      }
    }
    Ok(())
  } else {
    Err(eyre!("Server not exists name {}", server_id))
  }
}

pub async fn receive_uni_stream(mut uni_streams: quinn::IncomingUniStreams) -> Result<()> {
  while let Some(Ok(recv)) = uni_streams.next().await {
    let bytes = recv.read_to_end(1024).await?;
    let packet: Packet = ciborium::de::from_reader(&*bytes)?;
    tokio::spawn(async move {
      let packet_handler = SERVER.packet_handler.deref();
      if let Some(ControlFlow::Break(pkt)) = packet_handler(packet).await.log() {
        SERVER.handle_rest_pkt(pkt).await;
      }
    });
  }
  Ok(())
}
