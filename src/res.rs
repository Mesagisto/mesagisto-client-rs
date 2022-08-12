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

use crate::{db::DB, OptionExt};

pub trait PhotoHandler = Fn(&(Vec<u8>, IVec)) -> BoxFuture<Result<ArcStr>> + Send + Sync + 'static;

#[derive(Singleton, Default)]
pub struct Res {
  pub directory: LateInit<PathBuf>,
  pub handlers: DashMap<ArcStr, Vec<oneshot::Sender<PathBuf>>>,
  pub photo_url_resolver: LateInit<Box<dyn PhotoHandler>>,
}
impl Res {
  async fn poll(&self) -> notify::Result<()> {
    let (tx, mut rx) = channel(32);
    let mut watcher = RecommendedWatcher::new(move |res| {
      let tx_clone = tx.clone();
      smol::spawn(async move {
        tx_clone.send(res).await.unwrap();
      })
      .detach();
    })?;
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
        // Ok(Event {
        //   kind: EventKind::Remove(_),
        //   paths,
        //   ..
        // }) => {
        //   for filename in paths.iter().filter_map(|path| {
        //     path
        //       .file_name()
        //       .map(|str| ArcStr::from(str.to_string_lossy()))
        //   }) {
        //     if self.handlers.contains_key(&filename) {
        //       error!("Resource watch error: file deleted name:{}", filename);
        //       self.handlers.remove(&filename);
        //     }
        //   }
        // }
        Err(e) => error!("Resource watch error: {:?}", e),
        _ => {}
      }
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

  pub fn resolve_photo_url<F: PhotoHandler>(&self, f: F) {
    let h = Box::new(f);
    self.photo_url_resolver.init(h);
  }

  pub async fn get_photo_url<T>(&self, uid: T) -> Option<ArcStr>
  where
    T: AsRef<[u8]>,
  {
    let file_id = DB.get_image_id(&uid)?;
    let handler = &*self.photo_url_resolver;
    handler(&(uid.as_ref().to_vec(), file_id))
      .await
      .unwrap()
      .some()
  }
}

// #[cfg(test)]
// mod test {
//   #[test]
//   fn test() {
//   }
// }
