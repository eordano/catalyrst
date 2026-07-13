pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub big_endian: bool,
}

macro_rules! read_int {
    ($name:ident, $t:ty, $n:literal) => {
        pub fn $name(&mut self) -> $t {
            let mut b = [0u8; $n];
            b.copy_from_slice(&self.data[self.pos..self.pos + $n]);
            self.pos += $n;
            if self.big_endian {
                <$t>::from_be_bytes(b)
            } else {
                <$t>::from_le_bytes(b)
            }
        }
    };
}

impl<'a> Reader<'a> {
    pub const fn new(data: &'a [u8], big_endian: bool) -> Self {
        Reader {
            data,
            pos: 0,
            big_endian,
        }
    }

    read_int!(read_u8, u8, 1);
    read_int!(read_i8, i8, 1);
    read_int!(read_u16, u16, 2);
    read_int!(read_i16, i16, 2);
    read_int!(read_u32, u32, 4);
    read_int!(read_i32, i32, 4);
    read_int!(read_u64, u64, 8);
    read_int!(read_i64, i64, 8);
    read_int!(read_f32, f32, 4);
    read_int!(read_f64, f64, 8);

    pub fn read_bool(&mut self) -> bool {
        self.read_u8() != 0
    }

    pub fn read_bytes(&mut self, n: usize) -> &'a [u8] {
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        out
    }

    pub fn read_bytes_vec(&mut self, n: usize) -> Vec<u8> {
        self.read_bytes(n).to_vec()
    }

    pub fn read_cstr(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]).into_owned();
        if self.pos < self.data.len() {
            self.pos += 1;
        }
        s
    }

    pub fn read_aligned_string(&mut self) -> String {
        let length = self.read_i32();
        if length > 0 && (length as usize) <= self.data.len() - self.pos {
            let bytes = self.read_bytes(length as usize);
            let result = String::from_utf8_lossy(bytes).into_owned();
            self.align_stream(4);
            result
        } else {
            String::new()
        }
    }

    pub fn read_byte_array(&mut self) -> Vec<u8> {
        let length = self.read_i32() as usize;
        self.read_bytes_vec(length)
    }

    pub const fn align_stream(&mut self, alignment: usize) {
        let rem = self.pos % alignment;
        if rem != 0 {
            self.pos += alignment - rem;
        }
    }

    pub const fn position(&self) -> usize {
        self.pos
    }
}

pub struct Writer {
    pub buf: Vec<u8>,
    pub big_endian: bool,
}

macro_rules! write_int {
    ($name:ident, $t:ty) => {
        pub fn $name(&mut self, v: $t) {
            if self.big_endian {
                self.buf.extend_from_slice(&v.to_be_bytes());
            } else {
                self.buf.extend_from_slice(&v.to_le_bytes());
            }
        }
    };
}

impl Writer {
    pub const fn new(big_endian: bool) -> Self {
        Writer {
            buf: Vec::new(),
            big_endian,
        }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    write_int!(write_u8, u8);
    write_int!(write_i8, i8);
    write_int!(write_u16, u16);
    write_int!(write_i16, i16);
    write_int!(write_u32, u32);
    write_int!(write_i32, i32);
    write_int!(write_u64, u64);
    write_int!(write_i64, i64);
    write_int!(write_f32, f32);
    write_int!(write_f64, f64);

    pub fn write_bool(&mut self, v: bool) {
        self.write_u8(v as u8);
    }

    pub fn write_bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    pub fn write_cstr(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    pub fn write_aligned_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_i32(bytes.len() as i32);
        self.write_bytes(bytes);
        self.align_stream(4);
    }

    pub fn write_byte_array(&mut self, v: &[u8]) {
        self.write_i32(v.len() as i32);
        self.write_bytes(v);
    }

    pub fn align_stream(&mut self, alignment: usize) {
        let rem = self.buf.len() % alignment;
        if rem != 0 {
            self.buf.extend(std::iter::repeat_n(0u8, alignment - rem));
        }
    }

    pub const fn position(&self) -> usize {
        self.buf.len()
    }
}
