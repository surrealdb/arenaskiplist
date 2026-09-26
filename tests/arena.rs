use arenaskiplist::arena::Arena;
use arenaskiplist::node::NODE_ALIGNMENT;

#[test]
fn test_arena_alignment() {
    let arena = Arena::new(1024 * 1024);

    for size in [1, 3, 7, 8, 15, 16, 31, 32, 64, 128] {
        let offset = arena
            .alloc(size, NODE_ALIGNMENT, 0)
            .expect("allocation failed");
        assert_eq!(
            offset % NODE_ALIGNMENT,
            0,
            "offset {offset} must be 8-byte aligned"
        );
    }
}

#[test]
fn test_arena_reset() {
    let mut arena = Arena::new(4096);
    assert_eq!(arena.capacity(), 4096);
    assert!(arena.is_empty());

    // Fill arena
    let _ = arena.alloc(2048, 8, 0).unwrap();
    assert!(!arena.is_empty());
    assert!(arena.size() >= 2048);

    // Reset
    arena.reset();
    assert!(arena.is_empty());
    assert_eq!(arena.size(), 1);

    // Can allocate again
    let offset = arena.alloc(1024, 8, 0).unwrap();
    assert_eq!(offset, 8);
}

#[test]
fn test_arena_bounds() {
    let arena = Arena::new(1024);
    assert!(arena.alloc(2048, 8, 0).is_none());
    assert_eq!(arena.capacity(), 1024);
}
