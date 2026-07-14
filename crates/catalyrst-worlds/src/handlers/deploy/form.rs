use std::collections::BTreeMap;

use axum::extract::Multipart;
use axum::response::{IntoResponse, Response};
use bytes::{Bytes, BytesMut};

use catalyrst_types::deploy_form::{MAX_DEPLOY_FILES, MAX_DEPLOY_FILE_BYTES};

use crate::upload_limits;

use super::{err_one, MAX_UPLOAD_SIZE_BYTES};

pub(super) struct DeployForm {
    pub(super) fields: BTreeMap<String, String>,
    pub(super) files: Vec<Bytes>,
}

fn account_deploy_bytes(
    total_bytes: &mut usize,
    added: usize,
    bytes_lease: &mut upload_limits::InFlightBytesGuard,
    max_in_flight_bytes: u64,
) -> Result<(), Response> {
    match upload_limits::account_payload_bytes(
        total_bytes,
        added,
        MAX_UPLOAD_SIZE_BYTES,
        bytes_lease,
        max_in_flight_bytes,
    ) {
        Ok(()) => Ok(()),
        Err(upload_limits::PayloadAccountError::PayloadTooLarge) => {
            Err(err_one(upload_limits::PAYLOAD_TOO_LARGE_MESSAGE).into_response())
        }
        Err(upload_limits::PayloadAccountError::BudgetExhausted) => {
            tracing::warn!(
                total_bytes,
                in_flight = upload_limits::in_flight_upload_bytes(),
                max = max_in_flight_bytes,
                "POST /entities shed: aggregate in-flight upload budget exceeded"
            );
            Err(upload_limits::shed_response(
                upload_limits::BYTES_SHED_MESSAGE,
            ))
        }
    }
}

pub(super) async fn read_deploy_form(
    mut multipart: Multipart,
    bytes_lease: &mut upload_limits::InFlightBytesGuard,
    files_lease: &mut upload_limits::InFlightFilesGuard,
    max_in_flight_bytes: u64,
    max_in_flight_files: u64,
) -> Result<DeployForm, Response> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut files: Vec<Bytes> = Vec::new();
    let mut total_bytes: usize = 0;
    let mut part_count: usize = 0;
    let mut field_count: usize = 0;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return Err(err_one(format!("Failed to read multipart field: {e}")).into_response());
            }
        };
        let mut field = field;
        let name = field.name().unwrap_or("").to_string();

        part_count += 1;
        if part_count > upload_limits::MAX_MULTIPART_PARTS {
            return Err(err_one(upload_limits::TOO_MANY_PARTS_MESSAGE).into_response());
        }

        if field.file_name().is_some() {
            if files.len() >= MAX_DEPLOY_FILES {
                return Err(err_one(format!(
                    "deployment exceeds maximum of {MAX_DEPLOY_FILES} files"
                ))
                .into_response());
            }
            if !files_lease.try_resize(files.len() as u64 + 1, max_in_flight_files) {
                tracing::warn!(
                    request_files = files.len() + 1,
                    in_flight = upload_limits::in_flight_upload_files(),
                    max = max_in_flight_files,
                    "POST /entities shed: aggregate in-flight upload-file budget exceeded"
                );
                return Err(upload_limits::shed_response(
                    upload_limits::FILES_SHED_MESSAGE,
                ));
            }
            let mut buf = BytesMut::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len().saturating_add(chunk.len()) > MAX_DEPLOY_FILE_BYTES {
                            return Err(err_one(
                                "An uploaded file exceeds the maximum allowed size.",
                            )
                            .into_response());
                        }
                        account_deploy_bytes(
                            &mut total_bytes,
                            chunk.len(),
                            bytes_lease,
                            max_in_flight_bytes,
                        )?;
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(
                            err_one(format!("Failed to read file data: {e}")).into_response()
                        )
                    }
                }
            }
            files.push(buf.freeze());
        } else {
            field_count += 1;
            if field_count > upload_limits::MAX_MULTIPART_FIELDS {
                return Err(err_one(upload_limits::TOO_MANY_FIELDS_MESSAGE).into_response());
            }
            let mut buf = BytesMut::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len().saturating_add(chunk.len())
                            > upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES
                        {
                            return Err(
                                err_one(upload_limits::PAYLOAD_TOO_LARGE_MESSAGE).into_response()
                            );
                        }
                        account_deploy_bytes(
                            &mut total_bytes,
                            chunk.len(),
                            bytes_lease,
                            max_in_flight_bytes,
                        )?;
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(
                            err_one(format!("Failed to read field value: {e}")).into_response()
                        )
                    }
                }
            }
            let value = String::from_utf8_lossy(&buf).into_owned();
            fields.insert(name, value);
        }
    }

    Ok(DeployForm { fields, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::{json, Value};

    async fn multipart_from(parts: &[(&str, Option<&str>, &str)]) -> Multipart {
        use axum::extract::FromRequest;
        let boundary = "xyzgate";
        let mut body = String::new();
        for (name, filename, value) in parts {
            body.push_str(&format!("--{boundary}\r\n"));
            match filename {
                Some(f) => body.push_str(&format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )),
                None => {
                    body.push_str(&format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"))
                }
            }
            body.push_str(value);
            body.push_str("\r\n");
        }
        body.push_str(&format!("--{boundary}--\r\n"));
        let req = axum::http::Request::builder()
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        Multipart::from_request(req, &()).await.unwrap()
    }

    async fn response_json(resp: Response) -> (StatusCode, Value) {
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn read_deploy_form_grows_the_lease_from_parsed_payload_bytes() {
        let multipart = multipart_from(&[
            ("entityId", None, "bafy123"),
            ("file1", Some("a.txt"), "hello world"),
        ])
        .await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let form = read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            u64::MAX,
            u64::MAX,
        )
        .await
        .expect("form parses");
        assert_eq!(form.fields.get("entityId").unwrap(), "bafy123");
        assert_eq!(form.files.len(), 1);
        assert_eq!(bytes_lease.reserved(), 7 + 11);
        assert_eq!(files_lease.reserved(), 1);
    }

    #[tokio::test]
    async fn read_deploy_form_sheds_when_the_byte_budget_is_exhausted() {
        let multipart = multipart_from(&[("entityId", None, "bafy123")]).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let resp = match read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            0,
            u64::MAX,
        )
        .await
        {
            Err(resp) => resp,
            Ok(_) => panic!("must shed"),
        };
        let (status, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            json!({
                "error": "Service Unavailable",
                "message": "Server is buffering too many uploads, please retry shortly."
            })
        );
        assert_eq!(bytes_lease.reserved(), 0);
    }

    #[tokio::test]
    async fn read_deploy_form_sheds_when_the_file_budget_is_exhausted() {
        let multipart = multipart_from(&[
            ("entityId", None, "bafy123"),
            ("file1", Some("a.txt"), "hello world"),
        ])
        .await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let resp = match read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            u64::MAX,
            0,
        )
        .await
        {
            Err(resp) => resp,
            Ok(_) => panic!("must shed"),
        };
        let (status, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            json!({
                "error": "Service Unavailable",
                "message": "Server is buffering too many upload files, please retry shortly."
            })
        );
        assert_eq!(files_lease.reserved(), 0);
    }

    #[tokio::test]
    async fn read_deploy_form_caps_each_field_value_at_one_megabyte() {
        let big = "a".repeat(upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES + 1);
        let multipart = multipart_from(&[("authChain", None, big.as_str())]).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let resp = match read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            u64::MAX,
            u64::MAX,
        )
        .await
        {
            Err(resp) => resp,
            Ok(_) => panic!("oversized field must be rejected"),
        };
        let (status, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            json!({ "errors": ["The multipart request is too large."] })
        );
        assert!(bytes_lease.reserved() <= upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES as u64);

        let exact = "a".repeat(upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES);
        let multipart = multipart_from(&[("authChain", None, exact.as_str())]).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let form = read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            u64::MAX,
            u64::MAX,
        )
        .await
        .expect("1 MB field parses");
        assert_eq!(
            form.fields.get("authChain").unwrap().len(),
            upload_limits::MAX_MULTIPART_FIELD_VALUE_BYTES
        );
    }

    #[tokio::test]
    async fn read_deploy_form_rejects_more_than_one_hundred_fields() {
        let names: Vec<String> = (0..=upload_limits::MAX_MULTIPART_FIELDS)
            .map(|i| format!("f{i}"))
            .collect();
        let parts: Vec<(&str, Option<&str>, &str)> =
            names.iter().map(|n| (n.as_str(), None, "v")).collect();
        let multipart = multipart_from(&parts).await;
        let mut bytes_lease = upload_limits::reserve_in_flight();
        let mut files_lease = upload_limits::reserve_in_flight_files();
        let resp = match read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            u64::MAX,
            u64::MAX,
        )
        .await
        {
            Err(resp) => resp,
            Ok(_) => panic!("101st field must be rejected"),
        };
        let (status, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            json!({ "errors": ["The multipart request has too many fields."] })
        );
    }
}
