#![feature(fn_traits, trait_alias, box_syntax)]
use std::{fmt::Debug, ops::ControlFlow, sync::Arc};

use arcstr::ArcStr;
use cache::CACHE;
use cipher::CIPHER;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use data::Packet;
use db::DB;
use educe::Educe;
use futures::future::BoxFuture;
use net::NET;
use res::RES;
use server::SERVER;
use uuid::Uuid;

pub mod cache;
pub mod cipher;
pub mod data;
pub mod db;
pub mod error;
pub mod net;
pub mod quic;
pub mod res;
pub mod server;
pub mod tls;

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate singleton;
#[macro_use]
extern crate rust_i18n;
#[macro_use]
extern crate derive_builder;

i18n!("locales");

const NAMESPACE_MSGIST: Uuid = Uuid::from_u128(31393687336353710967693806936293091922);

#[cfg(test)]
#[test]
fn test_uuid() {
  assert_eq!(
    "179e3449-c41f-4a57-a763-59a787efaa52",
    NAMESPACE_MSGIST.to_string()
  );
  println!(
    "{}",
    Uuid::new_v5(&NAMESPACE_MSGIST, "test".as_bytes()).to_string()
  );
}

#[derive(Educe, Builder)]
#[educe(Default, Debug)]
#[builder(setter(into))]
pub struct MesagistoConfig {
  #[educe(Default = "default")]
  pub name: ArcStr,
  pub proxy: Option<ArcStr>,
  pub cipher_key: ArcStr,
  #[educe(Default = "127.0.0.1:0")]
  pub local_address: String,
  pub remote_address: Arc<DashMap<ArcStr, ArcStr>>,
}
impl MesagistoConfig {
  pub async fn apply(self) -> Result<()> {
    DB.init(self.name.some());
    CACHE.init();
    CIPHER.init(&self.cipher_key)?;
    RES.init().await;
    SERVER
      .init(&self.local_address, self.remote_address)
      .await?;
    NET.init(self.proxy);
    Ok(())
  }

  pub fn packet_handler<F>(resolver: F)
  where
    F: Fn(Packet) -> BoxFuture<'static, Result<ControlFlow<Packet>>> + Send + Sync + 'static,
  {
    let h = Box::new(resolver);
    SERVER.packet_handler.init(h);
  }
}

pub trait ResultExt<T, E> {
  fn ignore(self) -> Option<T>;
  fn log(self) -> Option<T>;
}
impl<T, E: Debug> ResultExt<T, E> for Result<T, E> {
  #[inline]
  fn ignore(self) -> Option<T> {
    match self {
      Ok(v) => Some(v),
      Err(_) => None,
    }
  }

  #[inline]
  fn log(self) -> Option<T> {
    match self {
      Ok(v) => Some(v),
      Err(e) => {
        error!("{:?}", e);
        None
      }
    }
  }
}

pub trait OkExt<E> {
  #[inline]
  fn ok(self) -> Result<Self, E>
  where
    Self: Sized,
  {
    Ok(self)
  }
}
impl<T, E> OkExt<E> for T {}

pub trait OptionExt {
  #[inline]
  fn some(self) -> Option<Self>
  where
    Self: Sized,
  {
    Some(self)
  }
  #[inline]
  fn some_ref(&self) -> Option<&Self>
  where
    Self: Sized,
  {
    Some(self)
  }
}
impl<T> OptionExt for T {}

pub trait EitherExt<A> {
  #[inline]
  fn to_left(self) -> either::Either<Self, A>
  where
    Self: Sized,
  {
    either::Either::Left(self)
  }
  #[inline]
  fn tl(self) -> either::Either<Self, A>
  where
    Self: Sized,
  {
    either::Either::Left(self)
  }
  #[inline]
  fn to_right(self) -> either::Either<A, Self>
  where
    Self: Sized,
  {
    either::Either::Right(self)
  }
  #[inline]
  fn r(self) -> either::Either<A, Self>
  where
    Self: Sized,
  {
    either::Either::Right(self)
  }
}
impl<T, A> EitherExt<A> for T {}
