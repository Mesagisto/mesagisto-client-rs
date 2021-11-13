#![feature(fn_traits)]
use once_cell::sync::OnceCell;

pub mod cache;
pub mod cipher;
pub mod data;
pub mod db;
pub mod error;
pub mod net;
pub mod res;
pub mod server;

#[macro_use]
extern crate singleton;

#[derive(Debug)]
pub struct LateInit<T> {
  cell: OnceCell<T>,
}

impl<T> LateInit<T> {
  pub fn init(&self, value: T) {
    assert!(self.cell.set(value).is_ok())
  }
}

impl<T> Default for LateInit<T> {
  fn default() -> Self {
    LateInit {
      cell: OnceCell::default(),
    }
  }
}

impl<T> std::ops::Deref for LateInit<T> {
  type Target = T;
  fn deref(&self) -> &T {
    self.cell.get().unwrap()
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

pub trait ResultExt<T,E> {
  fn ignore(self) -> Option<T>;
}
impl<T, E> ResultExt<T,E> for Result<T,E> {
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
    Some(&self)
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
