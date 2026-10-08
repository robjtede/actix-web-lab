//! Collection traits and implementations for common collection types.
//!
//! Use `List` to read a collection's length and elements. Use `MutableList` to append elements and
//! access them mutably.
//!
//! These traits are implemented for:
//!
//! - `Vec`
//! - `arrayvec::ArrayVec`
//! - `smallvec::SmallVec`
//! - `tinyvec::ArrayVec`
//!
//! Appending to a full fixed-capacity collection panics.
//!
//! # Usage
//!
//! Add `collectools` to your dependencies:
//!
//! ```toml
//! [dependencies]
//! collectools = "0.1.1"
//! ```
//!
//! ```rust
//! use collectools::{List, MutableList};
//!
//! fn append_value(values: &mut impl MutableList<i32>) {
//!     values.append(42);
//! }
//!
//! let mut values = Vec::new();
//!
//! append_value(&mut values);
//!
//! assert_eq!(List::len(&values), 1);
//! assert_eq!(List::get(&values, 0), Some(&42));
//! ```

#![allow(missing_docs)]

mod arrayvec;
mod smallvec;
mod tinyvec;
mod vec;

pub trait List<T> {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, idx: usize) -> Option<&T>;
}

pub trait MutableList<T>: List<T> {
    fn append(&mut self, element: T);

    fn get_mut(&mut self, idx: usize) -> Option<&mut T>;
}
