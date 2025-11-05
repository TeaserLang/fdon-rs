# fdon-rs: Fast Data Object Notation Parser for Rust

`fdon-rs` is a high-performance, zero-copy parser for the FDON (Fast Data Object Notation) format, implemented in Rust. It is designed for **read** speed and memory efficiency by leveraging the zero-copy principle and SIMD-optimized string search primitives.

> Since this project is experimental and has *sacrificed* many things for speed, it may be unstable and will have bugs.

## Features

- **Zero-Copy Parsing:** Borrows string and key slices directly from the input buffer (`&str`), eliminating heap allocations during parsing.

- **High Performance:** Utilizes the `memchr` library for SIMD-accelerated delimiter searching, resulting in extremely fast parsing speeds.

- **Minification Included:** Provides a utility function to automatically strip non-essential whitespace before parsing, adhering to the FDON philosophy.

- **Serde Integration:** Data types implement `serde::Serialize` for easy integration with JSON or other serialization formats.

## Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
fdon-rs = "0.1.0" # Use the latest version
serde = { version = "1.0", features = ["derive"] }
```
> **IMPORTANT PERFORMANCE NOTE:** Due to the low-level optimizations (SIMD, Zero-Copy) used, the parse speed will drastically decrease (approx. 10x) without compiler optimizations. Ensure to run benchmarks or measure performance using the `--release` flag (e.g., `cargo run --release`, `cargo test --release`).

## API Options (Choose your speed vs. safety)

`fdon-rs` provides two main parsing functions, allowing you to choose between memory safety (default) and maximum convenience/speed (for benchmarks).

### Option 1: Safe Zero-Copy (Recommended Default)

Use `parse_fdon_zero_copy_ref`. This function performs a standard zero-copy parse, borrowing data from your input string. You are responsible for managing the lifetime of the input data.

**This is the recommended approach for most applications.**
```rust
use fdon_rs::{minify_fdon, parse_fdon_zero_copy_ref};
use serde_json;

// 1. Read input data (from file or network)
let raw_data = "O { key : S\"value\", array: A [ N123, Btrue ] }";

// 2. Minify (removes whitespace)
let minified_data = minify_fdon(&raw_data);

// 3. Parse (Zero-Copy)
// 'minified_data' must live longer than 'value'
match parse_fdon_zero_copy_ref(&minified_data) {
    Ok(value) => {
        // 'value' is FdonValue<'_> and borrows from 'minified_data'
        println!("Parse successful!");
        
        // Convert to JSON
        let json = serde_json::to_string(&value).unwrap();
        println!("{}", json);
    }
    Err((msg, pos)) => {
        eprintln!("Error at position {}: {}", pos, msg);
    }
}
```

### Option 2: Static Parse (Benchmark / CLI)

Use `parse_fdon_zero_copy_static`. This function is built for **maximum speed and convenience** in short-lived applications (like this CLI or tests). It handles minification and parsing in one step, returning a `FdonValue<'static>`.

> **WARNING:** This function **WILL LEAK MEMORY** (one `String`) every time it is called. Do NOT use this in a long-running server.
```rust
use fdon_rs::parse_fdon_zero_copy_static;

// 1. Read input data
let raw_data = "O { key : S\"value\" }";

// 2. Minify and Parse (One step, leaks memory)
match parse_fdon_zero_copy_static(raw_data) {
    Ok(static_value) => {
        // 'static_value' is FdonValue<'static>
        println!("Parse successful (static)!");
    }
    Err((msg, pos)) => {
        eprintln!("Error at position {}: {}", pos, msg);
    }
}
```

## License

This project is licensed under the **AGPL-3.0-or-later**.