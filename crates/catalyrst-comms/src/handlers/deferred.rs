use axum::response::Response;

use crate::http::not_implemented;

pub async fn cast_any() -> Response {
    not_implemented("Cast 2.0 (RTMP / ingress): large surface, depends on LiveKit IngressClient; see TODO.md")
}
pub async fn scene_stream_access_put_delete() -> Response {
    not_implemented("scene-stream-access PUT/DELETE depends on Cast 2.0; see TODO.md")
}
