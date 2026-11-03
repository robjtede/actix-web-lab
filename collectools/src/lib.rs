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

mod arrayvec;
mod smallvec;
mod tinyvec;
mod vec;

/// Read access to an ordered collection of elements.
pub trait List<T> {
    /// Returns the number of elements in the collection.
    fn len(&self) -> usize;

    /// Returns `true` if the collection has no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a reference to the element at `idx`, or `None` if the index is out of bounds.
    fn get(&self, idx: usize) -> Option<&T>;
}

/// Mutable access to an ordered collection of elements.
///
/// # Examples
///
/// ```
/// use collectools::{List, MutableList};
///
/// let mut values = vec![1];
///
/// *MutableList::get_mut(&mut values, 0).unwrap() = 2;
/// MutableList::append(&mut values, 3);
///
/// assert_eq!(values, [2, 3]);
/// assert_eq!(List::get(&values, 2), None);
/// assert_eq!(MutableList::get_mut(&mut values, 2), None);
/// ```
pub trait MutableList<T>: List<T> {
    /// Adds an element to the end of the collection.
    ///
    /// # Panics
    ///
    /// The fixed-capacity implementations for `arrayvec::ArrayVec` and `tinyvec::ArrayVec`
    /// panic if the collection is full.
    fn append(&mut self, element: T);

    /// Returns a mutable reference to the element at `idx`, or `None` if the index is out of bounds.
    fn get_mut(&mut self, idx: usize) -> Option<&mut T>;
}
