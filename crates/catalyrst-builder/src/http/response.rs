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
