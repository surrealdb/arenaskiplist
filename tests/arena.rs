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

#[test]
fn test_chunked_arena_growth() {
    let arena = Arena::with_chunk_config(1024 * 1024, 4);
    assert!(arena.is_chunked());
    assert_eq!(arena.allocated_chunks(), 1);

    // Allocate 600KB (fits in chunk 0)
    let off1 = arena.alloc(600 * 1024, 8, 0).unwrap();
    assert_eq!(arena.allocated_chunks(), 1);

    // Allocate another 600KB (exceeds 1MB chunk 0, so should grow to chunk 1)
    let off2 = arena.alloc(600 * 1024, 8, 0).unwrap();
    assert_eq!(arena.allocated_chunks(), 2);

    // Write bytes to both chunks and verify integrity
    // SAFETY: Offset is within allocated chunk boundaries and exclusively accessed.
    let bytes1 = unsafe { arena.get_bytes_mut(off1, 100) };
    bytes1.fill(0xAA);

    // SAFETY: Offset is within allocated chunk boundaries and exclusively accessed.
    let bytes2 = unsafe { arena.get_bytes_mut(off2, 100) };
    bytes2.fill(0xBB);

    assert_eq!(arena.get_bytes(off1, 10), &[0xAA; 10]);
    assert_eq!(arena.get_bytes(off2, 10), &[0xBB; 10]);

    // Pointer offset roundtrip
    let ptr1 = arena.get_pointer(off1);
    assert_eq!(arena.get_pointer_offset(ptr1), off1);
    let ptr2 = arena.get_pointer(off2);
    assert_eq!(arena.get_pointer_offset(ptr2), off2);
}
