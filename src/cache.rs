use std::{panic, path::PathBuf, time::Duration};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use tracing::trace;

use crate::{
  data::{events::Event, Packet},
  net::NET,
  res::RES,
  server::SERVER,
  EitherExt,
};

#[derive(Singleton, Default)]
pub struct Cache {}

impl Cache {
  pub fn init(&self) {}

  pub async fn file(
    &self,
    id: &Vec<u8>,
    url: &Option<ArcStr>,
    address: &ArcStr,
  ) -> Result<PathBuf> {
    match url {
      Some(url) => self.file_by_url(id, url).await,
      None => self.file_by_uid(id, address).await,
    }
  }

  pub async fn file_by_uid(&self, uid: &Vec<u8>, address: &ArcStr) -> Result<PathBuf> {
    let uid_str: ArcStr = base64_url::encode(uid).into();
    trace!("Caching file by uid {}", uid_str);
    let path = RES.path(&uid_str);
    if path.exists() {
      trace!("File exists,return the path");
      return Ok(path);
    }
    let tmp_path = RES.tmp_path(&uid_str);
    if tmp_path.exists() {
      trace!("TmpFile exists,waiting for the file downloading");
      return Ok(RES.wait_for(&uid_str).await?);
    }
    trace!("TmpFile dont exist,requesting image url");
    let packet: Event = Event::RequestImage { id: uid.clone() };
    // fixme error handling
    let packet = Packet::from(packet.to_right())?;
    // fixme timeout check
    let response = SERVER.request(address, packet, SERVER.new_lib_header()?);
    let response = tokio::time::timeout(Duration::from_secs(5), response).await??;
    trace!("Get the image respond");
    let r_packet = Packet::from_cbor(&response.payload)?;
    match r_packet {
      either::Either::Right(event) => match event {
        Event::RespondImage { id, url } => self.file_by_url(&id, &url).await,
        _ => panic!("Not correct response"),
      },
      either::Either::Left(_) => panic!("Not correct response"),
    }
  }

  pub async fn file_by_url(&self, id: &Vec<u8>, url: &ArcStr) -> Result<PathBuf> {
    let id_str: ArcStr = base64_url::encode(id).into();
    let path = RES.path(&id_str);
    if path.exists() {
      return Ok(path);
    }

    let tmp_path = RES.tmp_path(&id_str);
    if tmp_path.exists() {
      Ok(RES.wait_for(&id_str).await?)
    } else {
      // fixme error handling
      NET.download(url, &tmp_path).await?;
      tokio::fs::rename(&tmp_path, &path).await?;
      Ok(path)
    }
  }

  pub async fn put_file(&self, id: &Vec<u8>, file: &PathBuf) -> Result<PathBuf> {
    let id_str: ArcStr = base64_url::encode(id).into();
    let path = RES.path(&id_str);
    tokio::fs::rename(&file, &path).await?;
    Ok(path)
  }
}
