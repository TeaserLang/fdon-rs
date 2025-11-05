use serde::Serialize;
use std::collections::HashMap;
use memchr::{memchr, memchr3};

// --- Cấu trúc dữ liệu ---

/// Represents a numeric value (Integer or Float)
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FdonNumber {
    Integer(i64),
    Float(f64),
}

/// Represents any FDON value (Zero-Copy)
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FdonValue<'a> {
    Null,
    Bool(bool),
    Number(FdonNumber),
    String(&'a str),
    Array(Vec<FdonValue<'a>>),
    Object(HashMap<&'a str, FdonValue<'a>>),
}

/// Parse Error type
pub type FdonParseError = (String, usize);
pub type ParseResult<'a, T> = Result<T, FdonParseError>;

// --- Minify Function (Đã tối ưu hóa bằng cách xử lý byte) ---

/// Minifies an FDON string, removing all whitespace outside of strings.
/// Tối ưu hóa bằng cách xử lý byte thay vì ký tự (char) để tăng tốc độ.
#[inline(always)] // Cưỡng chế inline vì nó được gọi trong hàm static
pub fn minify_fdon(input: &str) -> String {
    let input_bytes = input.as_bytes();
    let mut minified = Vec::with_capacity(input.len());
    let mut in_string = false;

    for &byte in input_bytes {
        match byte {
            b'"' => {
                in_string = !in_string;
                minified.push(byte);
            }
            // Skip these bytes (whitespace) ONLY if not in a string
            b' ' | b'\n' | b'\r' | b'\t' if !in_string => {
                // Skip
            }
            // Keep all other characters/bytes
            _ => {
                minified.push(byte);
            }
        }
    }
    // Tối ưu hóa bằng unsafe (an toàn vì input là &str hợp lệ)
    unsafe { String::from_utf8_unchecked(minified) }
}


// --- Parser ---

/// The FDON Parser, borrows the minified input data.
pub struct FdonParser<'a> {
    data: &'a [u8],
    index: usize,
}

impl<'a> FdonParser<'a> {
    /// Creates a new parser from a *minified* string slice.
    #[inline(always)]
    pub fn new(input: &'a str) -> Self {
        FdonParser {
            data: input.as_bytes(),
            index: 0,
        }
    }

    // --- Helper functions ---
    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.index).copied()
    }

    #[inline(always)]
    fn advance(&mut self) {
        self.index += 1;
    }

    /// Consumes an expected byte, or returns an error.
    #[inline(always)]
    fn consume(&mut self, char: u8) -> ParseResult<'a, ()> {
        if self.peek() == Some(char) {
            self.advance();
            Ok(())
        } else {
            let found = self.peek().map(|c| c as char).map(|c| c.to_string()).unwrap_or_else(|| "EOF".to_string());
            Err((
                format!("Expected '{}' but found '{}'", char as char, found),
                self.index,
            ))
        }
    }

    /// Starts the parsing process (Main function)
    #[inline(always)]
    pub fn parse(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        let value = self.parse_value()?;
        // Ensure we consumed the entire file
        if self.index != self.data.len() {
            Err((
                "Extra data detected at end of file".to_string(),
                self.index,
            ))
        } else {
            Ok(value)
        }
    }

    /// Parses a single FDON value
    #[inline(always)]
    fn parse_value(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        let type_char = self.peek().ok_or(("Unexpected EOF".to_string(), self.index))?;
        self.advance(); // Consume the type character

        match type_char {
            b'O' => self.parse_object(),
            b'A' => self.parse_array(),
            b'S' => self.parse_string(),
            b'N' => self.parse_number(),
            b'B' => self.parse_boolean(),
            b'U' => Ok(FdonValue::Null),
            _ => Err((
                format!("Unknown data type specifier '{}'", type_char as char),
                self.index - 1,
            )),
        }
    }

    /// Parses an Object: O{key:value,...}
    fn parse_object(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        let mut obj = HashMap::new();
        self.consume(b'{')?;

        while self.peek() != Some(b'}') {
            let key = self.parse_key()?;
            self.consume(b':')?;
            let value = self.parse_value()?;
            obj.insert(key, value);

            if self.peek() == Some(b',') {
                self.advance();
                if self.peek() == Some(b'}') {
                    return Err(("Trailing comma detected in object".to_string(), self.index));
                }
            } else if self.peek() != Some(b'}') {
                return Err(("Missing comma or '}' in object".to_string(), self.index));
            }
        }
        self.consume(b'}')?;
        Ok(FdonValue::Object(obj))
    }

    /// Parses a Key (reads until ':')
    #[inline(always)]
    fn parse_key(&mut self) -> ParseResult<'a, &'a str> {
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        match memchr(b':', remaining_data) {
            Some(pos) => {
                let end = self.index + pos;
                let key_slice = &self.data[start..end];
                self.index = end; // Stop *at* the ':'

                // TỐI ƯU HÓA UNSAFE: Bỏ qua xác thực UTF-8 vì FDON key phải là UTF-8
                unsafe {
                    Ok(std::str::from_utf8_unchecked(key_slice))
                }
            }
            None => Err(("EOF while reading key (':' not found)".to_string(), self.index)),
        }
    }

    /// Parses an Array: A[value,...]
    fn parse_array(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        let mut arr = Vec::new();
        self.consume(b'[')?;

        while self.peek() != Some(b']') {
            arr.push(self.parse_value()?);

            if self.peek() == Some(b',') {
                self.advance();
                if self.peek() == Some(b']') {
                    return Err(("Trailing comma detected in array".to_string(), self.index));
                }
            } else if self.peek() != Some(b']') {
                return Err(("Missing comma or ']' in array".to_string(), self.index));
            }
        }
        self.consume(b']')?;
        Ok(FdonValue::Array(arr))
    }

    /// Parses a String: S"..."
    #[inline(always)]
    fn parse_string(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        self.consume(b'"')?;
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        match memchr(b'"', remaining_data) {
            Some(pos) => {
                let end = self.index + pos;
                let val_slice = &self.data[start..end];
                
                // FDON rule: no escapes, no quotes inside.
                // (memchr nhanh hơn .contains)
                if memchr(b'"', val_slice).is_some() {
                     return Err(("Invalid quote (\") found inside string".to_string(), start));
                }

                self.index = end + 1; // Move *after* the closing "

                // TỐI ƯU HÓA UNSAFE: Bỏ qua xác thực UTF-8
                let val_str = unsafe { std::str::from_utf8_unchecked(val_slice) };
                
                Ok(FdonValue::String(val_str))
            }
            None => Err(("EOF while reading string ('\"' not found)".to_string(), start)),
        }
    }

    /// Parses a Number: N123 or N123.45
    #[inline(always)]
    fn parse_number(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        let end;
        match memchr3(b',', b'}', b']', remaining_data) {
            Some(pos) => {
                end = self.index + pos;
                self.index = end; // Stop *at* the delimiter
            }
            None => {
                // Number runs to the end of the file
                end = self.data.len();
                self.index = end;
            }
        }

        let num_slice = &self.data[start..end];
        if num_slice.is_empty() {
            return Err(("Empty number value".to_string(), self.index));
        }
        
        // --- TỐI ƯU HÓA: Kiểm tra số thực bằng byte SIMD-optimized ---
        let is_float = memchr(b'.', num_slice).is_some();
        
        // TỐI ƯU HÓA UNSAFE: Bỏ qua xác thực UTF-8 (vì số phải là UTF-8)
        let num_str = unsafe { std::str::from_utf8_unchecked(num_slice) };

        if is_float {
            let val: f64 = num_str.parse()
                .map_err(|e| (format!("Invalid float format: {}", e), start))?;
            Ok(FdonValue::Number(FdonNumber::Float(val)))
        } else {
            let val: i64 = num_str.parse()
                .map_err(|e| (format!("Invalid integer format: {}", e), start))?;
            Ok(FdonValue::Number(FdonNumber::Integer(val)))
        }
    }

    /// Parses a Boolean: Btrue or Bfalse
    #[inline(always)]
    fn parse_boolean(&mut self) -> ParseResult<'a, FdonValue<'a>> {
        if self.data.get(self.index..self.index + 4) == Some(b"true") {
            self.index += 4;
            Ok(FdonValue::Bool(true))
        } else if self.data.get(self.index..self.index + 5) == Some(b"false") {
            self.index += 5;
            Ok(FdonValue::Bool(false))
        } else {
            Err(("Invalid boolean value".to_string(), self.index))
        }
    }
}


// --- Public API Functions ---

// --- LỰA CHỌN 1: AN TOÀN (MẶC ĐỊNH) ---
/// (Recommended) Parses a minified FDON string using the zero-copy method.
///
/// The returned `FdonValue` borrows its string and key slices directly from the
/// input `minified_data`. Therefore, `minified_data` must outlive the returned value.
///
/// # Example
/// ```rust
/// // (Giả sử fdon_rs đã được import)
/// // let raw_data = "O{key:S\"value\"}";
/// // let minified_data = fdon_rs::minify_fdon(raw_data);
/// // match fdon_rs::parse_fdon_zero_copy_ref(&minified_data) {
/// //     Ok(value) => { /* ... */ },
/// //     Err(e) => { /* ... */ }
/// // }
/// ```
#[inline] // Inline hàm wrapper này
pub fn parse_fdon_zero_copy_ref<'a>(minified_data: &'a str) -> ParseResult<'a, FdonValue<'a>> {
    let mut parser = FdonParser::new(minified_data);
    parser.parse()
}

// --- LỰA CHỌN 2: TỐC ĐỘ TỐI ĐA (BENCHMARK/CLI) ---
/// (Benchmark/CLI Use) Minifies and parses FDON data, returning a `'static` value.
///
/// This function is extremely fast and convenient for short-lived processes (like CLIs or tests)
/// as it bypasses lifetime management by leaking the minified `String`.
///
/// **WARNING:** This function **WILL LEAK MEMORY** (one `String`) every time it is called.
/// Do NOT use this in a long-running server or application where memory leaks are critical.
///
/// # Example
/// ```rust
/// // (Giả sử fdon_rs đã được import)
/// // let raw_data = "O{key:S\"value\"}";
/// // match fdon_rs::parse_fdon_zero_copy_static(raw_data) {
/// //     Ok(static_value) => { /* ... */ },
/// //     Err(e) => { /* ... */ }
/// // }
/// ```
pub fn parse_fdon_zero_copy_static(input: &str) -> ParseResult<'static, FdonValue<'static>> {
    // 1. Minify (creates a new owned String, đã tối ưu hóa)
    let minified = minify_fdon(input);

    // 2. Leak the minified string to satisfy the 'static lifetime.
    let leaked_data: &'static str = Box::leak(minified.into_boxed_str());
    
    // 3. Parse (Zero-Copy)
    let mut parser = FdonParser::new(leaked_data);
    
    // Safety: Transmute không cần thiết nếu parser.parse() trả về đúng lifetime
    parser.parse()
}