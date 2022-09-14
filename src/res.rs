use std::{path::PathBuf, time::Duration};

use arcstr::ArcStr;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use futures::future::BoxFuture;
use lateinit::LateInit;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sled::IVec;
use tokio::sync::{mpsc::channel, oneshot};
use tracing::error;

use crate::{db::DB, ResultExt};

pub trait PhotoHandler = Fn(&(Vec<u8>, IVec)) -> BoxFuture<Result<ArcStr>> + Send + Sync + 'static;

#[derive(Singleton, Default)]
pub struct Res {
  pub directory: LateInit<PathBuf>,
  pub handlers: DashMap<ArcStr, Vec<oneshot::Sender<PathBuf>>>,
}
impl Res {
  async fn poll(&self) -> notify::Result<()> {
    let (tx, mut rx) = channel(32);
    let mut watcher = RecommendedWatcher::new(
      move |res| {
        let tx_clone = tx.clone();
        smol::spawn(async move {
          tx_clone.send(res).await.unwrap();
        })
        .detach();
      },
      notify::Config::default().with_poll_interval(Duration::from_secs(5)),
    )?;
    watcher.watch(self.directory.as_path(), RecursiveMode::NonRecursive)?;
    while let Some(res) = rx.recv().await {
      match res {
        Ok(Event {
          kind: EventKind::Create(_),
          paths,
          ..
        }) => {
          for path in paths {
            let file_name = ArcStr::from(path.file_name().unwrap().to_string_lossy());
            if let Some((.., handler_list)) = self.handlers.remove(&file_name) {
              for handler in handler_list {
                if handler.send(path.clone()).is_err() {
                  error!("Send a path to a closed handler")
                }
              }
            }
          }
        }
        Err(e) => error!("Resource watch error: {:?}", e),
        _ => {}
      }
      let mut for_remove = vec![];
      for entry in &self.handlers {
        let path = self.path(entry.key());
        if path.exists() {
          for_remove.push((entry.key().to_owned(),path));
        }
      }
      for_remove.into_iter().for_each(|v|{
        if let Some((.., handler_list)) = self.handlers.remove(&v.0) {
          for handler in handler_list {
            handler.send(v.1.to_owned()).log();
          }
        }
      });
    }
    Ok(())
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
    self
      .handlers
      .entry(id.clone())
      .or_insert(Vec::new())
      .push(sender);
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
    tokio::spawn(async { RES.poll().await });
  }

  pub fn put_image_id<U, F>(&self, uid: U, file_id: F)
  where
    U: AsRef<[u8]>,
    F: Into<IVec>,
  {
    DB.put_image_id(uid, file_id);
  }
}

// #[cfg(test)]
// mod test {
//   #[test]
//   fn test() {
//   }
// }
