use arenaskiplist::{Arena, Error, SkipList};

#[test]
fn test_insert_and_get() {
    let arena = Arena::with_capacity(4096 * 1024);
    let list = SkipList::new(arena);

    assert!(list.is_empty());
    assert_eq!(list.len(), 0);

    list.insert(b"apple", b"red").unwrap();
    list.insert(b"banana", b"yellow").unwrap();
    list.insert(b"cherry", b"dark red").unwrap();

    assert_eq!(list.len(), 3);
    assert!(!list.is_empty());

    assert_eq!(list.get_value(b"apple"), Some(&b"red"[..]));
    assert_eq!(list.get_value(b"banana"), Some(&b"yellow"[..]));
    assert_eq!(list.get_value(b"cherry"), Some(&b"dark red"[..]));
    assert_eq!(list.get_value(b"durian"), None);

    assert!(list.contains_key(b"apple"));
    assert!(!list.contains_key(b"durian"));
}

#[test]
fn test_duplicate_key_error() {
    let arena = Arena::with_capacity(1024 * 1024);
    let list = SkipList::new(arena);

    list.insert(b"key1", b"val1").unwrap();
    let err = list.insert(b"key1", b"val2").unwrap_err();
    assert_eq!(err, Error::RecordExists);
}

#[test]
fn test_arena_full() {
    // Small arena
    let arena = Arena::with_capacity(512);
    let list = SkipList::new(arena);

    let mut i = 0usize;
    let mut full = false;
    while i < 1000 {
        let k = format!("k_{i:04}");
        let v = format!("v_{i:04}");
        if let Err(e) = list.insert(k.as_bytes(), v.as_bytes()) {
            assert_eq!(e, Error::ArenaFull);
            full = true;
            break;
        }
        i += 1;
    }
    assert!(full, "expected ArenaFull error with tiny arena");
}
