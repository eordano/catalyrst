use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub data: Vec<String>,
}

impl ListResponse {
    pub fn new(data: Vec<String>) -> Self {
        Self { data }
    }
}
