use arenaskiplist::{Arena, SkipList};

#[test]
fn test_versioned_entries_ordering() {
    let arena = Arena::with_capacity(1024 * 1024);
    let list = SkipList::new(arena);

    // Insert multiple versions of the same key
    list.insert_with_version(b"foo", 10, b"val_10").unwrap();
    list.insert_with_version(b"foo", 30, b"val_30").unwrap();
    list.insert_with_version(b"foo", 20, b"val_20").unwrap();

    // Point get should return newest version (30)
    let entry = list.get(b"foo").expect("key found");
    assert_eq!(entry.version(), 30);
    assert_eq!(entry.value(), b"val_30");

    // Exact version lookups
    let v10 = list.get_with_version(b"foo", 10).unwrap();
    assert_eq!(v10.value(), b"val_10");
    let v20 = list.get_with_version(b"foo", 20).unwrap();
    assert_eq!(v20.value(), b"val_20");
    let v30 = list.get_with_version(b"foo", 30).unwrap();
    assert_eq!(v30.value(), b"val_30");
    assert!(list.get_with_version(b"foo", 40).is_none());

    // Snapshot reads (version <= max_version)
    assert_eq!(list.get_version_le(b"foo", 35).unwrap().version(), 30);
    assert_eq!(list.get_version_le(b"foo", 30).unwrap().version(), 30);
    assert_eq!(list.get_version_le(b"foo", 25).unwrap().version(), 20);
    assert_eq!(list.get_version_le(b"foo", 20).unwrap().version(), 20);
    assert_eq!(list.get_version_le(b"foo", 15).unwrap().version(), 10);
    assert_eq!(list.get_version_le(b"foo", 10).unwrap().version(), 10);
    assert!(list.get_version_le(b"foo", 5).is_none());
}
