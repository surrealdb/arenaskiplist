// Copyright (c) 2026 SurrealDB Ltd
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Deterministic Simulation Test (DST)
//!
//! Validates `SkipList` under randomized concurrent schedules against a canonical
//! `std::collections::BTreeMap` reference oracle.
//!
//! Run with:
//! ```bash
//! cargo test --test sim -- --nocapture
//! ARENASKIPLIST_SIM_SEED=12345678 cargo test --test sim -- --nocapture
//! ```

use arenaskiplist::{Arena, SkipList};
use std::collections::BTreeMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn get_seed() -> u64 {
    if let Ok(seed_str) = env::var("ARENASKIPLIST_SIM_SEED") {
        seed_str
            .parse::<u64>()
            .expect("ARENASKIPLIST_SIM_SEED must be a valid 64-bit integer")
    } else {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards");
        now.as_secs() ^ (now.subsec_nanos() as u64)
    }
}

fn generate_key(rng: &mut fastrand::Rng) -> String {
    let prefix_choice = rng.usize(0..4);
    let prefix = match prefix_choice {
        0 => "users:account:",
        1 => "users:profile:",
        2 => "orders:item:",
        _ => "data:",
    };
    let key_id = rng.u32(0..500);
    format!("{prefix}{key_id:06}")
}

#[test]
fn test_deterministic_simulation() {
    let seed = get_seed();
    println!("=== Running SkipList Deterministic Simulation Test with seed: {seed} ===");

    let mut master_rng = fastrand::Rng::with_seed(seed);
    let arena = Arena::with_capacity(64 * 1024 * 1024);
    let skl = Arc::new(SkipList::new(arena));
    let oracle = Arc::new(Mutex::new(BTreeMap::<String, (u64, u64)>::new()));

    const NUM_WORKERS: usize = 4;
    const OPS_PER_WORKER: usize = 1500;

    let handles: Vec<_> = (0..NUM_WORKERS)
        .map(|worker_id| {
            let skl = Arc::clone(&skl);
            let oracle = Arc::clone(&oracle);
            let worker_seed = master_rng.u64(..) ^ (worker_id as u64);

            std::thread::spawn(move || {
                let mut local_rng = fastrand::Rng::with_seed(worker_seed);

                for op_idx in 0..OPS_PER_WORKER {
                    let op = local_rng.usize(0..100);
                    let key = generate_key(&mut local_rng);
                    let version = ((worker_id * OPS_PER_WORKER) + op_idx + 1) as u64;

                    if op < 60 {
                        // 60% Insert with monotonic version
                        let val = local_rng.u64(..);
                        let mut o = oracle.lock().unwrap();
                        let _ =
                            skl.insert_with_version(key.as_bytes(), version, &val.to_be_bytes());
                        let current_entry = o.entry(key.clone()).or_insert((version, val));
                        if version >= current_entry.0 {
                            *current_entry = (version, val);
                        }
                    } else {
                        // 40% Point Get & verification
                        let o = oracle.lock().unwrap();
                        let expected = o.get(&key).copied();
                        drop(o);

                        let actual = skl.get(key.as_bytes()).map(|e| {
                            let mut buf = [0u8; 8];
                            buf.copy_from_slice(e.value());
                            (e.version(), u64::from_be_bytes(buf))
                        });

                        if let Some(expected_val) = expected {
                            let actual_val = actual.expect("key must exist in skiplist");
                            assert_eq!(
                                actual_val, expected_val,
                                "point get must match oracle latest entry"
                            );
                        }
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("simulation worker thread panicked");
    }

    // Post-simulation validation against oracle
    let o = oracle.lock().unwrap();
    println!(
        "Simulation completed. Validating {} unique keys against oracle...",
        o.len()
    );

    for (k, expected) in o.iter() {
        let entry = skl
            .get(k.as_bytes())
            .expect("oracle key missing in skiplist");
        let mut buf = [0u8; 8];
        buf.copy_from_slice(entry.value());
        let val = u64::from_be_bytes(buf);
        assert_eq!(entry.version(), expected.0, "version mismatch for key {k}");
        assert_eq!(val, expected.1, "value mismatch for key {k}");

        // Snapshot read validation at latest version
        let snap = skl
            .get_version_le(k.as_bytes(), expected.0)
            .expect("snapshot read failed");
        assert_eq!(snap.version(), expected.0);
    }

    // Range query validation against oracle
    let start_key = "orders:item:000100";
    let end_key = "users:profile:000400";
    let oracle_range: Vec<String> = o
        .range(start_key.to_string()..end_key.to_string())
        .map(|(k, _)| k.clone())
        .collect();

    let mut skl_range: Vec<String> = Vec::new();
    for e in skl.range(start_key.as_bytes()..end_key.as_bytes()) {
        let k = String::from_utf8(e.key().to_vec()).unwrap();
        if skl_range.last() == Some(&k) {
            continue; // deduplicate versions
        }
        skl_range.push(k);
    }

    assert_eq!(
        skl_range, oracle_range,
        "forward range scan must match oracle range"
    );

    let oracle_rev_range: Vec<String> = o
        .range(start_key.to_string()..end_key.to_string())
        .rev()
        .map(|(k, _)| k.clone())
        .collect();

    let mut skl_rev_range: Vec<String> = Vec::new();
    for e in skl.range(start_key.as_bytes()..end_key.as_bytes()).rev() {
        let k = String::from_utf8(e.key().to_vec()).unwrap();
        if skl_rev_range.last() == Some(&k) {
            continue;
        }
        skl_rev_range.push(k);
    }

    assert_eq!(
        skl_rev_range, oracle_rev_range,
        "reverse range scan must match oracle reverse range"
    );
}
