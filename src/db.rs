use crate::LateInit;
use arcstr::ArcStr;
use dashmap::DashMap;

#[derive(Singleton, Default)]
pub struct Db {
  image_db: LateInit<sled::Db>,
  // message id
  mid_db_map: DashMap<Vec<u8>, sled::Db>,

  db_name: LateInit<ArcStr>,
}
impl Db {
  pub fn init(&self, db_name: Option<ArcStr>) {
    let db_name = db_name.unwrap_or(ArcStr::from("default"));

    let options = sled::Config::default();
    let image_db_path = format!("db/{}/image", db_name);
    let image_db = options.path(image_db_path.as_str()).open().unwrap();
    self.image_db.init(image_db);

    self.db_name.init(db_name);
  }
  pub fn put_image_id(&self, uid: &ArcStr, file_id: &ArcStr) {
    self
      .image_db
      .insert(uid.as_bytes(), file_id.as_bytes())
      .unwrap();
  }
  pub fn get_image_id(&self, uid: &ArcStr) -> Option<ArcStr> {
    let r = self.image_db.get(uid.as_bytes()).unwrap()?;
    let file_id: ArcStr = String::from_utf8_lossy(&r).into();
    Some(file_id)
  }
  pub fn put_msg_id(
    &self,
    target: Vec<u8>,
    uid: Vec<u8>,
    id: Vec<u8>,
    reverse: bool,
  ) -> anyhow::Result<()> {
    let msg_id_db = self.mid_db_map.entry(target.clone()).or_insert_with(|| {
      let options = sled::Config::default();
      let msg_id_db_path = format!(
        "db/{}/msg-id/{}",
        *self.db_name,
        base64_url::encode(&target)
      );
      options.path(msg_id_db_path).open().unwrap()
    });
    msg_id_db.insert(&uid, id.clone())?;
    if reverse {
      msg_id_db.insert(&id, uid.clone())?;
    }
    Ok(())
  }
  pub fn get_msg_id(&self, target: &Vec<u8>, id: &Vec<u8>) -> anyhow::Result<Option<Vec<u8>>> {
    let msg_id_db = match self.mid_db_map.get(target) {
      Some(v) => v,
      None => return Ok(None),
    };
    let id = match msg_id_db.get(id)? {
      Some(v) => v,
      None => return Ok(None),
    }
    .to_vec();
    return Ok(Some(id));
  }
}
