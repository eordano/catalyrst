use std::cell::RefCell;
use std::fmt;

#[derive(Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),

    Bytes(Vec<u8>),
    Array(Vec<Value>),

    Map(Map),
}

#[derive(Clone)]
pub enum KeyStr {
    Static(&'static str),
    Owned(String),
}

impl KeyStr {
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            KeyStr::Static(s) => s,
            KeyStr::Owned(s) => s,
        }
    }
}

impl std::ops::Deref for KeyStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for KeyStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for KeyStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for KeyStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl From<&'static str> for KeyStr {
    #[inline]
    fn from(s: &'static str) -> Self {
        KeyStr::Static(s)
    }
}

impl From<String> for KeyStr {
    #[inline]
    fn from(s: String) -> Self {
        KeyStr::Owned(s)
    }
}

impl fmt::Display for KeyStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(PartialEq)]
pub struct Map(pub Vec<(KeyStr, Value)>);

struct Pools {
    map_vecs: Vec<Vec<(KeyStr, Value)>>,
    strings: Vec<String>,
}

impl Pools {
    const fn new() -> Self {
        Self {
            map_vecs: Vec::new(),
            strings: Vec::new(),
        }
    }
}

thread_local! {
    static POOLS: RefCell<Pools> = const { RefCell::new(Pools::new()) };
}

const MAP_POOL_CAP: usize = 1024;
const STR_POOL_CAP: usize = 4096;
const POOL_BUF_MAX_BYTES: usize = 4096;

#[inline]
fn pool_take_map_vec() -> Vec<(KeyStr, Value)> {
    POOLS
        .try_with(|p| p.borrow_mut().map_vecs.pop())
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[inline]
fn pool_take_string() -> Option<String> {
    POOLS
        .try_with(|p| p.borrow_mut().strings.pop())
        .ok()
        .flatten()
}

#[inline]
fn drain_value_into(v: Value, pools: &mut Pools) {
    match v {
        Value::Str(s) => push_string(s, pools),
        Value::Array(a) => {
            for x in a {
                drain_value_into(x, pools);
            }
        }
        Value::Map(m) => drain_map_into(m, pools),
        _ => {}
    }
}

#[inline]
fn push_key(k: KeyStr, pools: &mut Pools) {
    if let KeyStr::Owned(s) = k {
        push_string(s, pools);
    }
}

#[inline]
fn push_string(mut s: String, pools: &mut Pools) {
    let cap = s.capacity();
    if cap == 0 || cap > POOL_BUF_MAX_BYTES || pools.strings.len() >= STR_POOL_CAP {
        return;
    }
    s.clear();
    pools.strings.push(s);
}

fn drain_map_into(mut m: Map, pools: &mut Pools) {
    let cap = m.0.capacity();
    let too_big = cap * std::mem::size_of::<(KeyStr, Value)>() > POOL_BUF_MAX_BYTES;
    let mut v = std::mem::take(&mut m.0);
    for (k, val) in v.drain(..) {
        push_key(k, pools);
        drain_value_into(val, pools);
    }
    if cap != 0 && !too_big && pools.map_vecs.len() < MAP_POOL_CAP {
        pools.map_vecs.push(v);
    }
}

impl Default for Map {
    fn default() -> Self {
        Map::new()
    }
}

impl Clone for Map {
    fn clone(&self) -> Self {
        let mut v = pool_take_map_vec();
        v.reserve(self.0.len());
        for (k, val) in &self.0 {
            let k = match k {
                KeyStr::Static(s) => KeyStr::Static(s),
                KeyStr::Owned(s) => KeyStr::Owned(string_from_str(s)),
            };
            v.push((k, val.clone()));
        }
        Map(v)
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        if self.0.capacity() == 0 {
            return;
        }
        let cap = self.0.capacity();
        let too_big = cap * std::mem::size_of::<(KeyStr, Value)>() > POOL_BUF_MAX_BYTES;
        let mut v = std::mem::take(&mut self.0);
        let _ = POOLS.try_with(|p| {
            if let Ok(mut pools) = p.try_borrow_mut() {
                for (k, val) in v.drain(..) {
                    push_key(k, &mut pools);
                    drain_value_into(val, &mut pools);
                }
                if !too_big && pools.map_vecs.len() < MAP_POOL_CAP {
                    pools.map_vecs.push(v);
                }
            }
        });
    }
}

impl Map {
    pub fn new() -> Self {
        Map(pool_take_map_vec())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0
            .iter_mut()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    pub fn insert<K: IntoMapKey>(&mut self, key: K, val: impl Into<Value>) {
        let val = val.into();

        if let Some(idx) = self
            .0
            .iter()
            .position(|(k, _)| k.as_str() == key.as_key_str())
        {
            self.0[idx].1 = val;
            return;
        }
        self.0.push((key.into_map_key(), val));
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k.as_str() == key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(KeyStr, Value)> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub trait IntoMapKey {
    fn as_key_str(&self) -> &str;
    fn into_map_key(self) -> KeyStr;
}

impl IntoMapKey for String {
    #[inline]
    fn as_key_str(&self) -> &str {
        self.as_str()
    }
    #[inline]
    fn into_map_key(self) -> KeyStr {
        KeyStr::Owned(self)
    }
}

impl IntoMapKey for &'static str {
    #[inline]
    fn as_key_str(&self) -> &str {
        self
    }
    #[inline]
    fn into_map_key(self) -> KeyStr {
        KeyStr::Static(self)
    }
}

impl IntoMapKey for &String {
    #[inline]
    fn as_key_str(&self) -> &str {
        self.as_str()
    }
    #[inline]
    fn into_map_key(self) -> KeyStr {
        KeyStr::Owned(string_from_str(self))
    }
}

impl IntoMapKey for KeyStr {
    #[inline]
    fn as_key_str(&self) -> &str {
        self.as_str()
    }
    #[inline]
    fn into_map_key(self) -> KeyStr {
        self
    }
}

impl IntoMapKey for &KeyStr {
    #[inline]
    fn as_key_str(&self) -> &str {
        self.as_str()
    }
    #[inline]
    fn into_map_key(self) -> KeyStr {
        match self {
            KeyStr::Static(s) => KeyStr::Static(s),
            KeyStr::Owned(s) => KeyStr::Owned(string_from_str(s)),
        }
    }
}

#[inline]
fn string_from_str(s: &str) -> String {
    let mut buf = match pool_take_string() {
        Some(b) => b,
        None => return s.to_string(),
    };
    buf.reserve(s.len());
    buf.push_str(s);
    buf
}

impl Value {
    pub fn map() -> Value {
        Value::Map(Map::new())
    }

    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_map_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_map().and_then(|m| m.get(key))
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.as_map_mut().and_then(|m| m.get_mut(key))
    }

    pub fn insert<K: IntoMapKey>(&mut self, key: K, val: impl Into<Value>) {
        self.as_map_mut()
            .expect("insert on non-Map Value")
            .insert(key, val);
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Value { fn from(v: $t) -> Self { Value::Int(v as i64) } }
    )*};
}
from_int!(i8, u8, i16, u16, i32, u32, i64, u64, usize, isize);
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(v as f64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(string_from_str(v))
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Value::Str(string_from_str(v))
    }
}
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}
impl From<Map> for Value {
    fn from(v: Map) -> Self {
        Value::Map(v)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(x) => x.into(),
            None => Value::Null,
        }
    }
}

pub fn pptr(file_id: i64, path_id: i64) -> Value {
    let mut m = Map::new();
    m.insert("m_FileID", Value::Int(file_id));
    m.insert("m_PathID", Value::Int(path_id));
    Value::Map(m)
}

#[macro_export]
macro_rules! map {
    ( $( $k:expr => $v:expr ),* $(,)? ) => {{
        let mut m = $crate::value::Map::new();
        $( m.insert($k, $crate::value::Value::from($v)); )*
        $crate::value::Value::Map(m)
    }};
}

#[macro_export]
macro_rules! arr {
    ( $( $v:expr ),* $(,)? ) => {{
        $crate::value::Value::Array(vec![ $( $crate::value::Value::from($v) ),* ])
    }};
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Value::Array(a) => f.debug_list().entries(a.iter()).finish(),
            Value::Map(m) => f
                .debug_map()
                .entries(m.0.iter().map(|(k, v)| (k.as_str(), v)))
                .finish(),
        }
    }
}

impl fmt::Debug for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.0.iter().map(|(k, v)| (k.as_str(), v)))
            .finish()
    }
}
