use serde::Serialize;
use memchr::{memchr, memchr2, memchr3};

// --- TỐI ƯU HÓA "ALL-IN" ---
use bumpalo::{
    Bump, 
    collections::Vec as BumpVec,
    collections::String as BumpString, // 1. Dùng String của Bumpalo (Arena)
};
use hashbrown::HashMap as BumpHashMap;
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
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FdonValue<'a, 'bump> {
    Null,
    Bool(bool),
    Number(FdonNumber), // N...
    Timestamp(FdonNumber), // T... (dạng số)
    RawString(&'a str), // S"..."
    EscapedString(BumpString<'bump>), // SE"..."
    Date(&'a str), // D"..."
    Time(&'a str), // T"..." (dạng chuỗi)
    Array(BumpVec<'bump, FdonValue<'a, 'bump>>),
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
    let mut in_string_s = false; // Dùng cho S"..."
    let mut in_string_se = false; // Dùng cho SE"..."

    let mut i = 0;
    while i < input_bytes.len() {
        let byte = input_bytes[i];
        
        // Logic cho S"..." (Raw String)
        if byte == b'"' && !in_string_se {
             // Kiểm tra xem có phải là S" (start) hay D" hoặc T"
            if !in_string_s && i > 0 {
                let prefix = input_bytes[i-1];
                if prefix == b'S' || prefix == b'D' || prefix == b'T' {
                     in_string_s = true;
                }
            } else {
                 in_string_s = false;
            }
            minified.push(byte);
            i += 1;
            continue;
        }

        // Logic cho SE"..." (Escaped String)
        if byte == b'S' && i + 1 < input_bytes.len() && input_bytes[i+1] == b'E' {
             minified.push(b'S');
             minified.push(b'E');
             i += 2;
             
             // Tìm " mở đầu
             while i < input_bytes.len() && (input_bytes[i] == b' ' || input_bytes[i] == b'\t' || input_bytes[i] == b'\n' || input_bytes[i] == b'\r') {
                 i += 1;
             }
             if i < input_bytes.len() && input_bytes[i] == b'"' {
                 minified.push(b'"');
                 i += 1;
                 in_string_se = true;
                 
                 // Copy y hệt cho đến khi gặp " đóng (không bị escape)
                 while i < input_bytes.len() {
                     let se_byte = input_bytes[i];
                     minified.push(se_byte);
                     i += 1;
                     
                     if se_byte == b'\\' && i < input_bytes.len() {
                         // Nếu là escape (\\ hoặc \") thì copy cả ký tự sau
                         minified.push(input_bytes[i]);
                         i += 1;
                     } else if se_byte == b'"' {
                         // Dấu " không bị escape -> kết thúc SE
                         in_string_se = false;
                         break;
                     }
                 }
             }
             continue;
        }

        // Bỏ qua whitespace nếu không ở trong chuỗi nào cả
        if (byte == b' ' || byte == b'\n' || byte == b'\r' || byte == b'\t') && !in_string_s && !in_string_se {
            i += 1;
            continue;
        }

        // Giữ lại các ký tự khác
        minified.push(byte);
        i += 1;
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

    // --- Parse Logic ---
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
            
            b'S' => {
                // Check for SE"..." (Escaped String)
                if self.peek() == Some(b'E') {
                    self.advance(); // consume 'E'
                    self.parse_escaped_string()
                } else {
                    // S"..." (Raw String)
                    self.parse_raw_string(FdonValue::RawString)
                }
            }
            
            b'D' => self.parse_raw_string(FdonValue::Date), // D"..."
            
            b'T' => {
                // T (Đa hình): Có thể là T"..." (String) hoặc T... (Number)
                if self.peek() == Some(b'"') {
                    // T"..." -> String path
                    self.parse_raw_string(FdonValue::Time)
                } else {
                    // T... -> Number path
                    self.parse_number_internal()
                        .map(FdonValue::Timestamp) // Wrap in Timestamp
                }
            }

            b'N' => {
                // N... -> Number path
                self.parse_number_internal()
                    .map(FdonValue::Number) // Wrap in Number
            }

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
        let hasher = AHasher::new();
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

    // --- Parse Raw String (S"...", D"...", T"...") ---
    #[inline(always)]
    fn parse_raw_string(
        &mut self, 
        constructor: fn(&'a str) -> FdonValue<'a, 'bump>
    ) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        self.consume(b'"')?;
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        match memchr(b'"', remaining_data) {
            Some(pos) => {
                let end = self.index + pos;
                let val_slice = &self.data[start..end];
                
                self.index = end + 1; 

                // SỬA LỖI: std.str:: -> std::str::
                let val_str = unsafe { std::str::from_utf8_unchecked(val_slice) };
                
                Ok(constructor(val_str))
            }
            None => Err(("EOF while reading string ('\"' not found)".to_string(), start)),
        }
    }
    
    // --- Parse Escaped String (SE"...") ---
    fn parse_escaped_string(&mut self) -> ParseResult<'a, 'bump, FdonValue<'a, 'bump>> {
        self.consume(b'"')?;
        
        // Dùng String của Bumpalo để chứa kết quả unescape
        let mut unescaped_str = BumpString::new_in(self.arena);
        
        let mut start_chunk = self.index;

        // Tối ưu: Dùng memchr2 để tìm \ hoặc " (kết thúc)
        while let Some(pos) = memchr2(b'\\', b'"', &self.data[self.index..]) {
            
            let found_char = self.data[self.index + pos];
            
            if found_char == b'"' {
                // --- KẾT THÚC CHUỖI ---
                let end = self.index + pos;
                let chunk_slice = &self.data[start_chunk..end];
                
                // Thêm chunk cuối cùng (nếu có)
                if !chunk_slice.is_empty() {
                    unescaped_str.push_str(unsafe { std::str::from_utf8_unchecked(chunk_slice) });
                }
                
                self.index = end + 1; // Bỏ qua "
                return Ok(FdonValue::EscapedString(unescaped_str));
            }

            if found_char == b'\\' {
                // --- KÝ TỰ ESCAPE ---
                
                // 1. Thêm chunk an toàn trước đó
                let end_chunk = self.index + pos;
                let chunk_slice = &self.data[start_chunk..end_chunk];
                if !chunk_slice.is_empty() {
                    unescaped_str.push_str(unsafe { std::str::from_utf8_unchecked(chunk_slice) });
                }
                
                // 2. Bỏ qua dấu \
                self.index = end_chunk + 1;
                
                // 3. Xử lý ký tự được escape
                match self.peek() {
                    Some(b'n') => unescaped_str.push('\n'),
                    Some(b't') => unescaped_str.push('\t'),
                    Some(b'r') => unescaped_str.push('\r'),
                    Some(b'"') => unescaped_str.push('\"'),
                    Some(b'\\') => unescaped_str.push('\\'),
                    Some(other) => {
                        // Ký tự escape không hợp lệ, chỉ giữ lại ký tự đó
                        // (ví dụ: \a -> a)
                         unescaped_str.push(other as char);
                    }
                    None => return Err(("EOF after escape character '\\'".to_string(), self.index)),
                }
                
                // 4. Advance và reset chunk
                self.advance();
                start_chunk = self.index;
            }
        }

        // Nếu không tìm thấy " (lỗi EOF)
        Err(("EOF while reading escaped string ('\"' not found)".to_string(), self.index))
    }


    // --- Parse Number Internal (Sử dụng cho cả N và T) ---
    #[inline(always)]
    fn parse_number_internal(&mut self) -> ParseResult<'a, 'bump, FdonNumber> {
        let start = self.index;
        let remaining_data = &self.data[self.index..];

        let end;
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
            Ok(FdonNumber::Float(val))
        } else {
            let val: i64 = atoi::atoi(num_slice)
                .ok_or(("Invalid integer format or out of range".to_string(), start))?;
            Ok(FdonNumber::Integer(val))
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