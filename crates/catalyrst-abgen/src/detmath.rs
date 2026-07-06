// glibc (native) and the std-bundled libm (wasm) round transcendentals
// differently; every byte-path transcendental must call through these
// pure-Rust libm wrappers so both targets produce identical bits.

#[inline]
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[inline]
pub fn powf(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

#[inline]
pub fn sinf(x: f32) -> f32 {
    libm::sinf(x)
}

#[inline]
pub fn cosf(x: f32) -> f32 {
    libm::cosf(x)
}

#[inline]
pub fn log2(x: f64) -> f64 {
    libm::log2(x)
}

#[inline]
pub fn log2f(x: f32) -> f32 {
    libm::log2f(x)
}
