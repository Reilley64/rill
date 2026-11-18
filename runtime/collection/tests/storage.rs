use rill_collection::List;

#[test]
fn list_stores_actual_elements_and_forwards_owned_values_in_order() {
    // Neither Copy nor Clone. Its reference and owned allocation must survive storage.
    struct Value<'a>(&'a str, Box<u32>);
    let source = String::from("retained");
    let first = Value(&source, Box::new(19));
    let address = &*first.1 as *const u32;
    let mut list = List::from_elements([first, Value("second", Box::new(23))]);
    assert_eq!(list.len(), 2);
    assert_eq!(&*list.get(0).unwrap().1 as *const u32, address);
    assert_eq!(list.get(u64::MAX).map(|v| v.0), None);
    *list[0].1 = 29;
    assert_eq!(*list[0].1, 29);
    let mut iterator = list.into_owned_iter();
    let value = iterator.next().unwrap();
    assert_eq!(value.0, "retained");
    assert_eq!(&*value.1 as *const u32, address);
    assert_eq!(*value.1, 29);
    assert_eq!(iterator.next().unwrap().0, "second");
    assert!(iterator.next().is_none());
}

#[test]
fn borrowed_iterators_view_the_same_storage() {
    let mut list = List::from_elements([Box::new(1), Box::new(2)]);
    let mut read = list.iter();
    assert!(std::ptr::eq(read.next().unwrap(), list.get(0).unwrap()));
    assert_eq!(**read.next().unwrap(), 2);
    assert!(read.next().is_none());
    let mut write = list.iter_mut();
    **write.advance().unwrap() = 3;
    **write.advance().unwrap() = 4;
    assert!(write.advance().is_none());
    assert_eq!(**list.get(0).unwrap(), 3);
    assert_eq!(**list.get(1).unwrap(), 4);
}

#[test]
#[should_panic(expected = "List index out of bounds")]
fn index_does_not_return_absence() {
    let list = List::from_elements([1]);
    let _ = list[u64::MAX];
}

#[test]
fn element_operations_copy_and_dispose_only_initialized_storage() {
    let list = List::from_elements([Box::new(1), Box::new(2), Box::new(3)]);
    let copied = list.copy_with(|value| Box::new(**value));
    assert!(!std::ptr::eq(
        &**list.get(0).unwrap(),
        &**copied.get(0).unwrap()
    ));
    let mut disposed = Vec::new();
    copied.dispose_with(|value| disposed.push(*value));
    assert_eq!(disposed, [3, 2, 1]);
    let mut iterator = list.into_owned_iter();
    assert_eq!(*iterator.next().unwrap(), 1);
    disposed.clear();
    iterator.dispose_with(|value| disposed.push(*value));
    assert_eq!(disposed, [3, 2]);
}

#[test]
fn owned_iterator_keeps_unyielded_values_until_it_is_dropped() {
    use std::cell::Cell;
    struct CountDrop<'a>(&'a Cell<u32>);
    impl Drop for CountDrop<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let yielded = Cell::new(0);
    let retained = Cell::new(0);
    let mut iter =
        List::from_elements([CountDrop(&yielded), CountDrop(&retained)]).into_owned_iter();
    let first = iter.next().unwrap();
    assert_eq!(retained.get(), 0);
    drop(iter);
    assert_eq!(retained.get(), 1);
    assert_eq!(yielded.get(), 0);
    drop(first);
    assert_eq!(yielded.get(), 1);
}
