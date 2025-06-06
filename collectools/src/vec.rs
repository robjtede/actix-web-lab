use super::{List, MutableList};

impl<T> List<T> for Vec<T> {
    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn get(&self, idx: usize) -> Option<&T> {
        <[_]>::get(self, idx)
    }
}

impl<T> MutableList<T> for Vec<T> {
    fn append(&mut self, element: T) {
        self.push(element);
    }

    fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        <[_]>::get_mut(self, idx)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut vec = vec![];

        assert_eq!(0, List::len(&vec));
        assert!(List::is_empty(&vec));

        MutableList::append(&mut vec, 1);

        assert_eq!(1, List::len(&vec));
        assert!(!List::is_empty(&vec));

        assert_eq!(&1, List::get(&vec, 0).unwrap());
    }
}
