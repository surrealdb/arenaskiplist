use arenaskiplist::{Arena, SkipList};

#[test]
fn test_iteration_order() {
    let arena = Arena::with_capacity(1024 * 1024);
    let list = SkipList::new(arena);

    let keys = vec!["banana", "apple", "cherry", "elderberry", "date"];
    for &k in &keys {
        list.insert(k.as_bytes(), k.as_bytes()).unwrap();
    }

    let collected: Vec<String> = list
        .iter()
        .map(|e| String::from_utf8(e.key().to_vec()).unwrap())
        .collect();

    assert_eq!(
        collected,
        vec!["apple", "banana", "cherry", "date", "elderberry"]
    );

    // Reverse iteration
    let reverse: Vec<String> = list
        .iter()
        .rev()
        .map(|e| String::from_utf8(e.key().to_vec()).unwrap())
        .collect();

    assert_eq!(
        reverse,
        vec!["elderberry", "date", "cherry", "banana", "apple"]
    );
}

#[test]
fn test_range_bounds() {
    let arena = Arena::with_capacity(1024 * 1024);
    let list = SkipList::new(arena);

    for i in 0..10 {
        let k = format!("{i:02}");
        list.insert(k.as_bytes(), k.as_bytes()).unwrap();
    }

    // Range "02".."07"
    let collected: Vec<String> = list
        .range(&b"02"[..]..&b"07"[..])
        .map(|e| String::from_utf8(e.key().to_vec()).unwrap())
        .collect();
    assert_eq!(collected, vec!["02", "03", "04", "05", "06"]);

    // Inclusive range "02"..="07"
    let collected: Vec<String> = list
        .range(&b"02"[..]..=&b"07"[..])
        .map(|e| String::from_utf8(e.key().to_vec()).unwrap())
        .collect();
    assert_eq!(collected, vec!["02", "03", "04", "05", "06", "07"]);
}
