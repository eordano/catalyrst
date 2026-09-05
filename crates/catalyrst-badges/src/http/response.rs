use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Data<T: Serialize> {
    pub data: T,
}

impl<T: Serialize> Data<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}
