//! Collection traits.

#![allow(missing_docs)]

mod arrayvec;
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
