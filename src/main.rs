use std::env;
use std::fs;
use std::process;
use std::time::Instant;

// --- SỬA LỖI API ---
// Import API mới (chỉ dùng Arena) và các struct liên quan
use fdon_rs::{minify_fdon, FdonParseError, FdonValue, parse_fdon_zero_copy_arena};
// Import Bumpalo
use bumpalo::Bump;
// --- KẾT THÚC SỬA LỖI ---


// Hàm trợ giúp in lỗi (Giờ sẽ in lỗi trên file thô)
fn print_error((msg, pos): FdonParseError, raw_content: &str) -> ! {
    eprintln!("FDON Syntax Error: {} at position {}", msg, pos);
    
    // Chỉ in một phần của nội dung nếu nó quá dài
    const MAX_LEN: usize = 100;
    if raw_content.len() > MAX_LEN {
         let start = if pos > MAX_LEN / 2 { pos - MAX_LEN / 2 } else { 0 };
         let end = std::cmp::min(raw_content.len(), start + MAX_LEN);
         eprintln!("...{}...", &raw_content[start..end]);
         // Tính toán vị trí ^
         if pos >= start {
            eprintln!("{}^", " ".repeat(pos - start));
         } else {
            eprintln!("^ (Error at start)");
         }
    } else {
        eprintln!("{}", raw_content);
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
    let minified_content = minify_fdon(&content);
    let duration_minify = start_time_minify.elapsed();
    
    println!("--- FDON Process Timing ---");
    println!("Minified Data Size: {} bytes", minified_content.len());
    println!("Minify Time: {:.6} ms", duration_minify.as_secs_f64() * 1000.0);
    println!("{}", "-".repeat(30));


    // --- Bước 2: Parse (Sử dụng Arena) ---
    
    // TẠO ARENA
    let arena = Bump::new();
    
    let start_time_parse = Instant::now();
    
    // 'value' giờ đây mượn 'minified_content' (cho 'a) VÀ 'arena' (cho 'bump)
    let value: FdonValue<'_, '_> = match parse_fdon_zero_copy_arena(&minified_content, &arena) {
        Ok(v) => v,
        // In lỗi trên nội dung ĐÃ MINIFY (vì index lỗi là trên file đó)
        Err(e) => print_error(e, &minified_content),
    };

    let duration_parse = start_time_parse.elapsed(); 

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
    // (Lưu ý: Thời gian Parse này KHÔNG bao gồm Minify)
    println!("🚀 Parse Time (Arena, Zero-Copy): {:.6} ms", duration_parse_ms);
    println!("⚡ Serialize Time (minified): {:.6} ms", duration_serialize_ms);
    println!("Total Time (Parse + Serialize): {:.6} ms", duration_parse_ms + duration_serialize_ms);
    println!("{}", "-".repeat(30));

    // Arena sẽ tự động được giải phóng khi 'arena' ra khỏi scope
}