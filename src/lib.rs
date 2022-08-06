#![feature(fn_traits, trait_alias, backtrace)]
use arcstr::ArcStr;
use cache::CACHE;
use cipher::CIPHER;
use color_eyre::eyre::Result;
use dashmap::DashMap;
use db::DB;
use educe::Educe;
use futures::future::BoxFuture;
use net::NET;
use res::RES;
use server::SERVER;
use sled::IVec;

pub mod cache;
pub mod cipher;
pub mod data;
pub mod db;
pub mod error;
pub mod net;
mod pool;
pub mod res;
pub mod server;

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate singleton;
#[macro_use]
extern crate rust_i18n;
i18n!("locales");

type Handler = dyn Fn(&(Vec<u8>, IVec)) -> BoxFuture<Result<ArcStr>> + Send + Sync + 'static;

#[derive(Educe)]
#[educe(Default)]
pub struct MesagistoConfig {
  #[educe(Default = "default")]
  pub name: ArcStr,
  pub proxy: Option<ArcStr>,
  pub cipher_key: ArcStr,
  #[educe(Default = "nats://nats.mesagisto.org:4222")]
  pub nats_address: ArcStr,

  pub nats_pool: DashMap<ArcStr, ArcStr>,
  pub photo_url_resolver: Option<Box<Handler>>,
}
impl MesagistoConfig {
  pub fn builder() -> MesagistoConfigBuilder {
    MesagistoConfigBuilder::new()
  }

  pub async fn apply(self) -> Result<()> {
    DB.init(self.name.some());
    CACHE.init();
    CIPHER.init(&self.cipher_key)?;
    RES.init().await;
    RES
      .photo_url_resolver
      .init(self.photo_url_resolver.unwrap());
    SERVER.init(&self.nats_address).await?;
    NET.init(self.proxy);
    Ok(())
  }
}
#[derive(Default)]
pub struct MesagistoConfigBuilder {
  config: MesagistoConfig,
}
impl MesagistoConfigBuilder {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn name(mut self, name: impl Into<ArcStr>) -> Self {
    self.config.name = name.into();
    self
  }

  pub fn proxy(mut self, proxy: Option<ArcStr>) -> Self {
    self.config.proxy = proxy;
    self
  }

  pub fn cipher_key(mut self, key: impl Into<ArcStr>) -> Self {
    self.config.cipher_key = key.into();
    self
  }

  pub fn nats_address(mut self, address: impl Into<ArcStr>) -> Self {
    self.config.nats_address = address.into();
    self
  }

  pub fn photo_url_resolver<F>(mut self, resolver: F) -> Self
  where
    F: Fn(&(Vec<u8>, IVec)) -> BoxFuture<Result<ArcStr>> + Send + Sync + 'static,
  {
    let h = Box::new(resolver);
    self.config.photo_url_resolver = Some(h);
    self
  }

  pub fn build(self) -> MesagistoConfig {
    self.config
  }
}

// R refers to <Return>
pub trait RunExt<R> {
  // let is a keyword in rust,so...let's use ret
  fn run<F: FnOnce(Self) -> R>(self, f: F) -> R
  where
    Self: Sized,
  {
    f(self)
  }
  // fn run_ref<F: FnOnce(&Self) -> R>(&self, f: F) -> R {
  //     f(self)
  // }
  // fn run_mut<F: FnOnce(&mut Self) -> R>(&mut self, f: F) -> R {
  //     f(self)
  // }
}

impl<T, R> RunExt<R> for T {}

pub trait ResultExt<T, E> {
  fn ignore(self) -> Option<T>;
}
impl<T, E> ResultExt<T, E> for Result<T, E> {
  #[inline]
  fn ignore(self) -> Option<T> {
    match self {
      Ok(v) => Some(v),
      Err(_) => None,
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

pub trait LogResultExt<T> {
  fn log_if_error(self, message: &str) -> Option<T>;
}

impl<T> LogResultExt<T> for color_eyre::eyre::Result<T> {
  #[inline(always)]
  fn log_if_error(self, message: &str) -> Option<T> {
    match self {
      Ok(v) => Some(v),
      Err(e) => {
        tracing::error!(
          "{}, ErrorType {} Backtrace {:#?}",
          message,
          e,
          e.backtrace()
        );
        None
      }
    }
  }
}
