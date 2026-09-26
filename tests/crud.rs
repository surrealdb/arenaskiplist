use arenaskiplist::{Arena, Inserter, SkipList};

#[test]
fn test_empty_and_large_keys() {
    let arena = Arena::with_capacity(4 * 1024 * 1024);
    let list = SkipList::new(arena);

    // Empty key & value
    list.insert(b"", b"").unwrap();
    assert_eq!(list.get_value(b""), Some(&b""[..]));

    // Large key (1KB) and large value (16KB)
    let large_key = vec![0xAA; 1024];
    let large_val = vec![0xBB; 16384];

    list.insert(&large_key, &large_val).unwrap();
    assert_eq!(list.get_value(&large_key), Some(large_val.as_slice()));
}

#[test]
fn test_inserter_equivalence() {
    let arena1 = Arena::with_capacity(16 * 1024 * 1024);
    let list1 = SkipList::new(arena1);

    let arena2 = Arena::with_capacity(16 * 1024 * 1024);
    let list2 = SkipList::new(arena2);
    let mut inserter = Inserter::new();

    for i in 0..2000u64 {
        let k = i.to_be_bytes();
        list1.insert(&k, &k).unwrap();
        list2.insert_with_inserter(&k, &k, &mut inserter).unwrap();
    }

    assert_eq!(list1.len(), list2.len());

    for (e1, e2) in list1.iter().zip(list2.iter()) {
        assert_eq!(e1.key(), e2.key());
        assert_eq!(e1.value(), e2.value());
        assert_eq!(e1.version(), e2.version());
    }
}

#[test]
fn test_range_scan_edge_cases() {
    let arena = Arena::with_capacity(4 * 1024 * 1024);
    let list = SkipList::new(arena);

    for i in 10..20u64 {
        let k = i.to_be_bytes();
        list.insert(&k, &k).unwrap();
    }

    // Empty range
    let empty: Vec<_> = list
        .range(&15u64.to_be_bytes()[..]..&15u64.to_be_bytes()[..])
        .collect();
    assert!(empty.is_empty());

    // Unbounded range
    let all: Vec<_> = list.range::<[u8], _>(..).collect();
    assert_eq!(all.len(), 10);

    // Start bound beyond max
    let none_after: Vec<_> = list.range(&25u64.to_be_bytes()[..]..).collect();
    assert!(none_after.is_empty());

    // End bound before min
    let none_before: Vec<_> = list.range(..&5u64.to_be_bytes()[..]).collect();
    assert!(none_before.is_empty());
}
