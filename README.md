# fdon-rs: Fast Data Object Notation Parser for Rust

`fdon-rs` is a high-performance, **zero-copy** parser for the FDON (Fast Data Object Notation) format, implemented in Rust. It is designed for **read** speed and memory efficiency by leveraging the zero-copy principle and SIMD-optimized string search primitives.

> Since this project is experimental and has *sacrificed* many things for speed, it may be unstable and will have bugs.

## Features

- **Zero-Copy Parsing (Arena-based):** Borrows string and key slices directly from the input buffer (`&str`), eliminating heap allocations during parsing. Internal Array and Object structures are allocated within a **Bumpalo memory arena** for optimal bulk deallocation.

- **High Performance ("All-In" Optimization):**
    * Utilizes the `memchr` library for SIMD-accelerated delimiter searching.
    * Implements **Hashbrown** for HashMaps and **AHash** for the Hasher.
    * Uses **Bumpalo** (memory arena) to allocate internal collections, allowing for extremely fast, collective memory freeing when the arena goes out of scope.

- **Minification Included:** Provides a utility function to automatically strip non-essential whitespace before parsing, adhering to the FDON philosophy.

- **Serde Integration:** Data types implement `serde::Serialize` for easy integration with JSON or other serialization formats.

## Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
fdon-rs = "0.2.0" # Use the latest version
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

> **IMPORTANT PERFORMANCE NOTE:** Due to the low-level optimizations (SIMD, Zero-Copy) used, the parse speed will drastically decrease (approx. 10x) without compiler optimizations. Ensure to run benchmarks or measure performance using the `--release` flag (e.g., `cargo run --release`, `cargo test --release`).

## Main API: Arena Zero-Copy (Recommended)

`fdon-rs` now provides a single, main API built around a **Bumpalo Arena**. This approach combines memory safety (since the Arena is safely dropped) with maximum speed (due to bulk memory allocation).

Use `parse_fdon_zero_copy_arena`. You are required to create and pass a **Bumpalo arena** to the parsing function.

```rust
use fdon_rs::{minify_fdon, parse_fdon_zero_copy_arena};
use bumpalo::Bump;
use serde_json;

// 1. Read input data (from file or network)
let raw_data = "O { key : S\"value\", array: A [ N123, Btrue ] }";

// 2. Minify (removes whitespace)
let minified_data = minify_fdon(&raw_data);

// 3. Initialize the Memory Arena
let arena = Bump::new();

// 4. Parse (Zero-Copy within the Arena)
// 'value' borrows from 'minified_data' (for string slices) and 'arena' (for Array/Object structures)
match parse_fdon_zero_copy_arena(&minified_data, &arena) {
    Ok(value) => {
        // 'value' is FdonValue<'_, '_>
        println!("Parse successful!");
        
        // Convert to JSON
        let json = serde_json::to_string(&value).unwrap();
        println!("{}", json);
    }
    Err((msg, pos)) => {
        eprintln!("Error at position {}: {}", pos, msg);
    }
}
// The Arena and all memory allocated within it are automatically deallocated here.
```

## License

This project is licensed under the **Apache 2.0 License**.