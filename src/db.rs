use std::convert::TryInto;

use crate::{LateInit, OkExt, OptionExt};
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
  #[inline]
  pub fn put_msg_id_0(&self, target: &i64, uid: &i32, id: &i32) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.to_be_bytes().to_vec(),
      true,
    )
  }
  // no reverse
  #[inline]
  pub fn put_msg_id_ir_0(&self, target: &i64, uid: &i32, id: &i32) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.to_be_bytes().to_vec(),
      false,
    )
  }
  #[inline]
  pub fn put_msg_id_1(&self, target: &i64, uid: &Vec<u8>, id: &i32) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.clone(),
      id.to_be_bytes().to_vec(),
      true,
    )
  }
  #[inline]
  pub fn put_msg_id_ir_1(&self, target: &i64, uid: &Vec<u8>, id: &i32) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.clone(),
      id.to_be_bytes().to_vec(),
      false,
    )
  }
  #[inline]
  pub fn put_msg_id_2(&self, target: &i64, uid: &i32, id: &Vec<u8>) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.clone(),
      true,
    )
  }
  #[inline]
  pub fn put_msg_id_ir_2(&self, target: &i64, uid: &i32, id: &Vec<u8>) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.clone(),
      false,
    )
  }
  #[inline]
  pub fn put_msg_id_3(&self, target: &u64, uid: &u64, id: &Vec<u8>) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.clone(),
      true,
    )
  }
  #[inline]
  pub fn put_msg_id_ir_3(&self, target: &u64, uid: &u64, id: &Vec<u8>) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.clone(),
      false,
    )
  }
  #[inline]
  pub fn put_msg_id_4(&self, target: &u64, uid: &u64, id: &u64) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.to_be_bytes().to_vec(),
      true,
    )
  }
  // no reverse
  #[inline]
  pub fn put_msg_id_ir_4(&self, target: &u64, uid: &i32, id: &u64) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.to_be_bytes().to_vec(),
      id.to_be_bytes().to_vec(),
      false,
    )
  }
  #[inline]
  pub fn put_msg_id_5(&self, target: &u64, uid: &Vec<u8>, id: &u64) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.clone(),
      id.to_be_bytes().to_vec(),
      true,
    )
  }
  #[inline]
  pub fn put_msg_id_ir_5(&self, target: &u64, uid: &Vec<u8>, id: &u64) -> anyhow::Result<()> {
    self.put_msg_id(
      target.to_be_bytes().to_vec(),
      uid.clone(),
      id.to_be_bytes().to_vec(),
      false,
    )
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
  #[inline]
  pub fn get_msg_id_1(&self, target: &i64, id: &Vec<u8>) -> anyhow::Result<Option<i32>> {
    let be_bytes = match self.get_msg_id(&target.to_be_bytes().to_vec(), id)? {
      Some(v) => match v.len() {
        4 => v,
        _ => return Ok(None),
      },
      None => return Ok(None),
    };
    i32::from_be_bytes(be_bytes.try_into().unwrap()).some().ok()
  }
  #[inline]
  pub fn get_msg_id_2(&self, target: &i64, id: &Vec<u8>) -> anyhow::Result<Option<Vec<u8>>> {
    self.get_msg_id(&target.to_be_bytes().to_vec(), id)
  }
  #[inline]
  pub fn get_msg_id_3(&self, target: &u64, id: &Vec<u8>) -> anyhow::Result<Option<u64>> {
    let be_bytes = match self.get_msg_id(&target.to_be_bytes().to_vec(), id)? {
      Some(v) => match v.len() {
        8 => v,
        _ => return Ok(None),
      },
      None => return Ok(None),
    };
    u64::from_be_bytes(be_bytes.try_into().unwrap()).some().ok()
  }
  #[inline]
  pub fn get_msg_id_4(&self, target: &u64, id: &Vec<u8>) -> anyhow::Result<Option<Vec<u8>>> {
    self.get_msg_id(&target.to_be_bytes().to_vec(), id)
  }
}
