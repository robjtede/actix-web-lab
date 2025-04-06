use tinyvec::Array;
use tinyvec::ArrayVec;

use super::List;
use super::MutableList;

impl<A: Array> List<A::Item> for ArrayVec<A> {
    fn len(&self) -> usize {
        ArrayVec::len(self)
    }

    fn get(&self, idx: usize) -> Option<&A::Item> {
        <[_]>::get(self, idx)
    }
}

impl<A: Array> MutableList<A::Item> for ArrayVec<A> {
    fn append(&mut self, element: A::Item) {
        self.push(element);
    }

    fn get_mut(&mut self, idx: usize) -> Option<&mut A::Item> {
        <[_]>::get_mut(self, idx)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut vec = ArrayVec::<[_; 8]>::new();

        assert_eq!(0, List::len(&vec));
        assert!(List::is_empty(&vec));

        MutableList::append(&mut vec, 1);

        assert_eq!(1, List::len(&vec));
        assert!(!List::is_empty(&vec));

        assert_eq!(&1, List::get(&vec, 0).unwrap());
    }
}
