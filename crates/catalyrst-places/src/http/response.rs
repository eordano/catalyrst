use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiData<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> ApiData<T> {
    pub fn ok(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiDataTotal<T: Serialize> {
    pub ok: bool,
    pub data: Vec<T>,
    pub total: i64,
}

impl<T: Serialize> ApiDataTotal<T> {
    pub fn ok(data: Vec<T>, total: i64) -> Self {
        Self {
            ok: true,
            data,
            total,
        }
    }
}
