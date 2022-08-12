use std::env;

use anyhow::*;
use fs_extra::{copy_items, dir::CopyOptions};

fn main() -> Result<()> {
  // 这里告诉 cargo 如果 /res/ 目录中的任何内容发生了变化，就重新运行脚本
  println!("cargo:rerun-if-changed=res/*");

  let out_dir = env::var("OUT_DIR")?;
  let mut copy_options = CopyOptions::new();
  copy_options.overwrite = true;
  let mut paths_to_copy = Vec::new();
  paths_to_copy.push("res/");
  copy_items(&paths_to_copy, out_dir, &copy_options)?;

  Ok(())
}
