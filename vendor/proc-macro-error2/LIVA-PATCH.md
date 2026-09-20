# Local Rust compatibility patch

Source: crates.io proc-macro-error2 2.0.1, checksum
`11ec05c52be0a07b08061f7dd003e7d7092e0472bc731b4af7bb1ef876109802`.
Copied from the locally cached published source on 2026-09-17; Cargo cache was not modified.
MIT/Apache-2.0 licenses and upstream tests are retained.

Only source change: `src/lib.rs` uses `pub extern crate proc_macro;` instead of
`extern crate proc_macro;`. The existing public re-export requires public visibility
(Rust E0365, rust-lang/rust#127909). No lint suppression or API removal.

The root crates.io patch selects this copy for teloxide -> aquamarine.
Remove the patch and this directory when a verified upstream release incorporating
this fix can be adopted. Offline resolution currently has no newer cached candidate.
