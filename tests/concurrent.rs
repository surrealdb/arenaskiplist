use arenaskiplist::{Arena, SkipList};
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn test_concurrent_inserts() {
    let arena = Arena::with_capacity(32 * 1024 * 1024);
    let list = Arc::new(SkipList::new(arena));

    const NUM_THREADS: usize = 8;
    const PER_THREAD: usize = 2000;
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let list = Arc::clone(&list);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            for i in 0..PER_THREAD {
                let val = (t * PER_THREAD + i) as u64;
                let k = val.to_be_bytes();
                list.insert(&k, &k).unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(list.len(), NUM_THREADS * PER_THREAD);

    // Verify all items are present
    for t in 0..NUM_THREADS {
        for i in 0..PER_THREAD {
            let val = (t * PER_THREAD + i) as u64;
            let k = val.to_be_bytes();
            assert_eq!(list.get_value(&k), Some(&k[..]));
        }
    }
}

#[test]
fn test_concurrent_reads_and_writes() {
    let arena = Arena::with_capacity(32 * 1024 * 1024);
    let list = Arc::new(SkipList::new(arena));

    // Prepopulate
    for i in 0..1000u64 {
        list.insert(&i.to_be_bytes(), &i.to_be_bytes()).unwrap();
    }

    const WRITERS: usize = 4;
    const READERS: usize = 4;
    let barrier = Arc::new(Barrier::new(WRITERS + READERS));
    let mut handles = Vec::new();

    for t in 0..WRITERS {
        let list = Arc::clone(&list);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            for i in 0..1000u64 {
                let val = 1000 + (t as u64 * 1000) + i;
                let k = val.to_be_bytes();
                let _ = list.insert(&k, &k);
            }
        }));
    }

    for _ in 0..READERS {
        let list = Arc::clone(&list);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            for i in 0..1000u64 {
                let k = i.to_be_bytes();
                assert_eq!(list.get_value(&k), Some(&k[..]));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}
