use std::{path::PathBuf, time::Duration};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use lateinit::LateInit;
use sled::IVec;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{db::DB, ResultExt};

#[derive(Singleton, Default)]
pub struct Res {
  pub directory: LateInit<PathBuf>,
  pub handlers: DashMap<ArcStr, Vec<oneshot::Sender<PathBuf>>>,
}
impl Res {
  async fn poll(&self) {
    let _: JoinHandle<_> = tokio::spawn(async {
      let mut interval = tokio::time::interval(Duration::from_millis(200));
      loop {
        let mut for_remove = vec![];
        for entry in &RES.handlers {
          let path = RES.path(entry.key());
          if path.exists() {
            for_remove.push((entry.key().to_owned(), path));
          }
        }
        for_remove.into_iter().for_each(|v| {
          if let Some((.., handler_list)) = RES.handlers.remove(&v.0) {
            for handler in handler_list {
              handler.send(v.1.to_owned()).log();
            }
          }
        });
        interval.tick().await;
      }
    });
  }

  pub fn path(&self, id: &ArcStr) -> PathBuf {
    let mut path = self.directory.clone();
    path.push(id.as_str());
    path
  }

  pub fn tmp_path(&self, id: &ArcStr) -> PathBuf {
    let mut path = self.directory.clone();
    path.push(format!("{}.tmp", id));
    path
  }

  pub async fn wait_for(&self, id: &ArcStr) -> Result<PathBuf> {
    let (sender, receiver) = oneshot::channel();
    self.handlers.entry(id.clone()).or_default().push(sender);
    let path = tokio::time::timeout(Duration::from_secs_f32(13.0), receiver).await??;
    Ok(path)
  }

  pub async fn init(&self) {
    let path = {
      let mut dir = std::env::temp_dir();
      dir.push("mesagisto");
      dir
    };
    tokio::fs::create_dir_all(path.as_path()).await.unwrap();
    self.directory.init(path);
    RES.poll().await;
  }

  pub fn put_image_id<U, F>(&self, uid: U, file_id: F)
  where
    U: AsRef<[u8]>,
    F: Into<IVec>,
  {
    DB.put_image_id(uid, file_id);
  }
}
