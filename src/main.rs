use std::env;
use std::fs;
use std::process;
use std::time::Instant;

// Import crate (sử dụng tên từ Cargo.toml)
// Chúng ta sẽ import các hàm/struct công khai từ src/lib.rs
// (Sử dụng cả hai hàm API công khai)
use fdon_rs::{minify_fdon, FdonParseError, FdonValue, parse_fdon_zero_copy_static};

// Hàm trợ giúp in lỗi
fn print_error((msg, pos): FdonParseError, minified_content: &str) -> ! {
    eprintln!("FDON Syntax Error: {} at position {}", msg, pos);
    
    // Chỉ in một phần của nội dung nếu nó quá dài
    const MAX_LEN: usize = 100;
    if minified_content.len() > MAX_LEN {
         let start = if pos > MAX_LEN / 2 { pos - MAX_LEN / 2 } else { 0 };
         let end = std::cmp::min(minified_content.len(), start + MAX_LEN);
         eprintln!("...{}...", &minified_content[start..end]);
         eprintln!("{}^", " ".repeat(pos - start));
    } else {
        eprintln!("{}", minified_content);
        eprintln!("{}^", " ".repeat(pos));
    }
    
    process::exit(1);
}

fn main() {
    // --- Argument handling ---
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <filename>.fdon", args[0]);
        process::exit(1);
    }
    let filename = &args[1];

    // --- Read file ---
    let content = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: File not found or read error for '{}': {}", filename, e);
            process::exit(1);
        }
    };

    // --- Bước 1: Minify (Đo thời gian riêng) ---
    let start_time_minify = Instant::now();
    // Chúng ta chạy minify riêng để lấy kích thước và thời gian, 
    // nhưng hàm `parse_fdon_zero_copy_static` sẽ chạy lại nó.
    // (Trong benchmark thực tế, chúng ta sẽ chỉ gọi hàm static)
    let minified_content_for_stats = minify_fdon(&content);
    let duration_minify = start_time_minify.elapsed();
    
    println!("--- FDON Process Timing ---");
    println!("Minified Data Size: {} bytes", minified_content_for_stats.len());
    println!("Minify Time: {:.6} ms", duration_minify.as_secs_f64() * 1000.0);
    println!("{}", "-".repeat(30));


    // --- Bước 2: Parse (Sử dụng hàm static TỐC ĐỘ CAO) ---
    // Hàm này tự động minify VÀ parse, chấp nhận rò rỉ RAM
    let start_time_parse = Instant::now();
    
    let value: FdonValue<'static> = match parse_fdon_zero_copy_static(&content) {
        Ok(v) => v,
        // Nếu lỗi, chúng ta cần minified_content để in lỗi
        Err(e) => print_error(e, &minified_content_for_stats),
    };

    let duration_parse = start_time_parse.elapsed(); // Thời gian này bao gồm cả Minify + Parse

    // --- Serialization và In kết quả ---
    let start_time_serialize = Instant::now();

    let json_output = serde_json::to_string(&value)
        .unwrap_or_else(|e| format!("Error serializing to JSON: {}", e));

    let duration_serialize = start_time_serialize.elapsed();

    // --- Print Results ---
    println!("--- Result (JSON) ---");
    let sample = json_output.chars().take(100).collect::<String>();
    println!("Sample (first 100 chars): {}", sample);
    println!("Total JSON size: {} bytes", json_output.len());
    println!("{}", "-".repeat(30));
    
    // Tính toán và in tốc độ
    let duration_parse_ms = duration_parse.as_secs_f64() * 1000.0;
    let duration_serialize_ms = duration_serialize.as_secs_f64() * 1000.0;
    
    println!("--- FDON Process Timing (Summary) ---");
    // (Lưu ý: Thời gian Parse này bao gồm cả Minify)
    println!("🚀 Parse Time (Minify + Parse, Zero-Copy Static): {:.6} ms", duration_parse_ms);
    println!("⚡ Serialize Time (minified): {:.6} ms", duration_serialize_ms);
    println!("Total Time (Parse + Serialize): {:.6} ms", duration_parse_ms + duration_serialize_ms);
    println!("{}", "-".repeat(30));
}