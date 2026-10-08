use planora_kernel::{Arena, Id};
use std::collections::HashSet;

#[test]
fn arena_push_index_and_iter() {
    let mut a: Arena<&str> = Arena::new();
    assert!(a.is_empty());
    let x = a.push("x");
    let y = a.push("y");
    assert_eq!(a.len(), 2);
    assert_eq!(a[x], "x");
    assert_eq!(a[y], "y");
    let all: Vec<_> = a.iter().map(|(id, v)| (id.index(), *v)).collect();
    assert_eq!(all, vec![(0, "x"), (1, "y")]);
}

#[test]
fn arena_index_mut() {
    let mut a: Arena<i32> = Arena::new();
    let id = a.push(1);
    a[id] = 5;
    assert_eq!(a[id], 5);
}

#[test]
fn ids_are_copy_eq_hash_ord() {
    let a: Id<String> = Id::from_index(3);
    let b = a;
    assert_eq!(a, b);
    assert!(Id::<String>::from_index(1) < Id::<String>::from_index(2));
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
}
