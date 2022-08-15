use std::{
  collections::HashMap,
  net::{SocketAddr, SocketAddrV4, SocketAddrV6},
  sync::Arc, ops::{Deref, ControlFlow},
};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use futures::StreamExt;
use trust_dns_resolver::{
  config::{ResolverConfig, ResolverOpts},
  TokioAsyncResolver,
};
use url::Host;

use crate::{server, server::SERVER, ResultExt, data::Packet};

pub async fn init(
  server: &server::Server,
  local: &str,
  remotes: HashMap<ArcStr, ArcStr>,
) -> Result<()> {
  // let local_socket = local.parse::<SocketAddr>()?;

  let local_socket = local.parse::<SocketAddr>()?;
  let mut endpoint = quinn::Endpoint::client(local_socket)?;
  endpoint.set_default_client_config(crate::tls::client_config().await?);
  server.endpoint.init(endpoint);
  for (name, remote) in remotes {
    connect(server, name, remote).await.log();
  }
  Ok(())
}
pub async fn connect(server: &server::Server, name: ArcStr, remote: ArcStr) -> Result<()> {
  let endpoint = &*server.endpoint;
  info!("{}", t!("log.connecting", address = &remote));
  let remote_url = url::Url::parse(&remote)?;

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
    server_name = "localhost"
  }
  let new_conn = endpoint.connect(remote_socket, server_name)?.await?;
  info!("{}", t!("log.connected"));
  server.remote_endpoints.insert(name, new_conn.connection);
  tokio::spawn(async move {
    receive_uni_stream(new_conn.uni_streams).await.unwrap();
  });
  Ok(())
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
