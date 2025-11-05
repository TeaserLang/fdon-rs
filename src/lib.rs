use serde::Serialize;
use memchr::{memchr, memchr3};

// --- TỐI ƯU HÓA "ALL-IN" ---
use bumpalo::{
    Bump, 
    collections::Vec as BumpVec // 1. Dùng Vec của Bumpalo (Arena)
};
// 2. Dùng HashMap của HASHBROWN (Không phải std)
use hashbrown::HashMap as BumpHashMap;
// 3. Dùng Hasher của AHASH
use ahash::RandomState as AHasher;
// --- KẾT THÚC KẾ HOẠCH ---

// --- Cấu trúc dữ liệu ---

/// Represents a numeric value (Integer or Float)
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FdonNumber {
    Integer(i64),
    Float(f64),
}

/// Represents any FDON value (Zero-Copy)
/// CHÚ Ý: 'bump (lifetime) giờ đây là một phần của kiểu Allocator
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FdonValue<'a, 'bump> {
    Null,
    Bool(bool),
    Number(FdonNumber),
    String(&'a str),
    // Tối ưu hóa Array (Thành công từ Kế hoạch Hybrid)
    Array(BumpVec<'bump, FdonValue<'a, 'bump>>),
    // Tối ưu hóa Object (Kế hoạch "All-In")
    // Kiểu đầy đủ: HashMap<Key, Value, Hasher, Allocator>
    Object(BumpHashMap<&'a str, FdonValue<'a, 'bump>, AHasher, &'bump Bump>),
}

/// Parse Error type
pub type FdonParseError = (String, usize);
pub type ParseResult<'a, 'bump, T> = Result<T, FdonParseError>;

// --- Minify Function (Giữ nguyên) ---

#[inline(always)]
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
            b' ' | b'\n' | b'\r' | b'\t' if !in_string => {
                // Skip
            }
            _ => {
                minified.push(byte);
            }
        }
    }
    unsafe { String::from_utf8_unchecked(minified) }
}


// --- Parser ---

pub struct FdonParser<'a, 'bump> {
    data: &'a [u8],
    index: usize,
    arena: &'bump Bump, 
}

impl<'a, 'bump> FdonParser<'a, 'bump> {
    #[inline(always)]
    pub fn new(input: &'a str, arena: &'bump Bump) -> Self {
        FdonParser {
            data: input.as_bytes(),
            index: 0,
            arena,
        }
    }

    // --- Helpers (Không đổi) ---
    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.index).copied()
    }

    #[inline(always)]
    fn advance(&mut self) {
        self.index += 1;
    }

    #[inline(always)]
    fn consume(&mut self, char: u8) -> ParseResult<'a, 'bump, ()> {
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

    // --- Parse Logic (Không đổi) ---
    #[inline(always)]
    pub fn parse(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        let value = self.parse_value()?;
        if self.index != self.data.len() {
            Err((
                "Extra data detected at end of file".to_string(),
                self.index,
            ))
        } else {
            Ok(value)
        }
    }

    #[inline(always)]
    fn parse_value(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        let type_char = self.peek().ok_or(("Unexpected EOF".to_string(), self.index))?;
        self.advance(); 

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

    // --- Parse Object (TỐI ƯU HÓA "ALL-IN") ---
    fn parse_object(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        // 1. Lấy AHasher
        let hasher = AHasher::new();
        // 2. Tạo HashMap BÊN TRONG ARENA (self.arena) VỚI AHasher
        let mut obj = BumpHashMap::with_hasher_in(hasher, self.arena);
        
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

    // --- Parse Key (Không đổi) ---
    #[inline(always)]
    fn parse_key(&mut self) -> ParseResult<'a, 'bump, &'a str> {
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        match memchr(b':', remaining_data) {
            Some(pos) => {
                let end = self.index + pos;
                let key_slice = &self.data[start..end];
                self.index = end; 

                unsafe {
                    Ok(std::str::from_utf8_unchecked(key_slice))
                }
            }
            None => Err(("EOF while reading key (':' not found)".to_string(), self.index)),
        }
    }

    // --- Parse Array (Đã tối ưu với BumpVec) ---
    fn parse_array(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        let mut arr = BumpVec::new_in(self.arena);
        
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

    // --- Parse String (Không đổi) ---
    #[inline(always)]
    fn parse_string(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        self.consume(b'"')?;
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        match memchr(b'"', remaining_data) {
            Some(pos) => {
                let end = self.index + pos;
                let val_slice = &self.data[start..end];
                
                self.index = end + 1; 

                let val_str = unsafe { std::str::from_utf8_unchecked(val_slice) };
                
                Ok(FdonValue::String(val_str))
            }
            None => Err(("EOF while reading string ('\"' not found)".to_string(), start)),
        }
    }

    // --- Parse Number (SỬA LỖI TYPO!!!) ---
    #[inline(always)]
    fn parse_number(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        let end;
        // SỬA LỖI: remaining_muta -> remaining_data
        match memchr3(b',', b'}', b']', remaining_data) {
            Some(pos) => {
                end = self.index + pos;
                self.index = end; 
            }
            None => {
                end = self.data.len();
                self.index = end;
            }
        }

        let num_slice = &self.data[start..end];
        if num_slice.is_empty() {
            return Err(("Empty number value".to_string(), self.index));
        }
        
        let is_float = memchr(b'.', num_slice).is_some();

        if is_float {
            let val: f64 = fast_float::parse(num_slice)
                .map_err(|e| (format!("Invalid float format: {}", e), start))?;
            Ok(FdonValue::Number(FdonNumber::Float(val)))
        } else {
            let val: i64 = atoi::atoi(num_slice)
                .ok_or(("Invalid integer format or out of range".to_string(), start))?;
            Ok(FdonValue::Number(FdonNumber::Integer(val)))
        }
    }

    // --- Parse Boolean (Không đổi) ---
    #[inline(always)]
    fn parse_boolean(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
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


// --- Public API Functions (Chỉ dùng Arena) ---

#[inline]
pub fn parse_fdon_zero_copy_arena<'a, 'bump>(
    minified_data: &'a str,
    arena: &'bump Bump
) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
    let mut parser = FdonParser::new(minified_data, arena);
    parser.parse()
}