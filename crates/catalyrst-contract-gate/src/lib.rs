pub mod pg;

use std::collections::BTreeMap;

use axum::body::{to_bytes, Body};
use axum::http::Request;
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

pub use catalyrst_crypto::{create_simple_auth_chain, Wallet};

const BODY_LIMIT: usize = 64 * 1024 * 1024;
const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

pub fn assert_usable_spec(service: &str, spec: &Value) {
    let version = spec
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{service}: spec has no `openapi` version field"));
    assert!(
        version.starts_with('3'),
        "{service}: unexpected OpenAPI version {version}"
    );
    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{service}: spec has no `paths` object"));
    assert!(
        !paths.is_empty(),
        "{service}: the spec declares zero paths, so every contract gate reading it would pass \
         having checked nothing"
    );
    for (path, item) in paths {
        assert!(
            path.starts_with('/'),
            "{service}: path key {path} is not rooted at /"
        );
        let item = item
            .as_object()
            .unwrap_or_else(|| panic!("{service}: path item {path} is not an object"));
        let mut methods = 0usize;
        for method in METHODS {
            let Some(op) = item.get(method) else { continue };
            methods += 1;
            let responses = op
                .get("responses")
                .and_then(Value::as_object)
                .unwrap_or_else(|| {
                    panic!(
                        "{service}: {} {path} declares no responses",
                        method.to_uppercase()
                    )
                });
            assert!(
                !responses.is_empty(),
                "{service}: {} {path} declares an empty response set",
                method.to_uppercase()
            );
        }
        assert!(
            methods > 0,
            "{service}: path {path} declares no HTTP method"
        );
    }
}

pub fn test_wallet(seed: u8) -> Wallet {
    let mut key = [0u8; 32];
    key[0] = 1;
    key[31] = seed;
    Wallet::from_hex(&hex_encode(&key)).expect("test wallet")
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn signed_fetch_headers(wallet: &Wallet, method: &str, path: &str) -> Vec<(String, String)> {
    signed_fetch_headers_with(wallet, method, path, "{}")
}

pub fn signed_fetch_headers_with(
    wallet: &Wallet,
    method: &str,
    path: &str,
    metadata: &str,
) -> Vec<(String, String)> {
    let ts = chrono::Utc::now().timestamp_millis().to_string();
    let payload = format!("{}:{}:{}:{}", method, path, ts, metadata).to_lowercase();
    let chain = create_simple_auth_chain(wallet, &payload).expect("auth chain");
    vec![
        ("x-identity-auth-chain-0".into(), chain[0].to_string()),
        ("x-identity-auth-chain-1".into(), chain[1].to_string()),
        ("x-identity-timestamp".into(), ts),
        ("x-identity-metadata".into(), metadata.into()),
    ]
}

pub struct MultipartPart {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl MultipartPart {
    pub fn field(name: &str, value: &str) -> Self {
        Self {
            name: name.into(),
            filename: None,
            content_type: None,
            data: value.as_bytes().to_vec(),
        }
    }

    pub fn file(name: &str, filename: &str, content_type: &str, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            content_type: Some(content_type.into()),
            data,
        }
    }
}

pub fn multipart_body(parts: &[MultipartPart]) -> (Vec<u8>, String) {
    let boundary = "contractgateboundary7345";
    let mut out = Vec::new();
    for p in parts {
        out.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        match &p.filename {
            Some(f) => out.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    p.name, f
                )
                .as_bytes(),
            ),
            None => out.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n", p.name).as_bytes(),
            ),
        }
        if let Some(ct) = &p.content_type {
            out.extend_from_slice(format!("Content-Type: {}\r\n", ct).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&p.data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    (out, format!("multipart/form-data; boundary={}", boundary))
}

pub struct Case {
    method: String,
    spec_path: String,
    path: String,
    query: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<(Vec<u8>, String)>,
    expect: u16,
}

impl Case {
    pub fn new(method: &str, spec_path: &str) -> Self {
        Self {
            method: method.to_uppercase(),
            spec_path: spec_path.into(),
            path: spec_path.into(),
            query: None,
            headers: Vec::new(),
            body: None,
            expect: 200,
        }
    }

    pub fn path(mut self, path: &str) -> Self {
        self.path = path.into();
        self
    }

    pub fn query(mut self, query: &str) -> Self {
        self.query = Some(query.into());
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn bearer(self, token: &str) -> Self {
        let value = format!("Bearer {}", token);
        self.header("authorization", &value)
    }

    pub fn signed(mut self, wallet: &Wallet) -> Self {
        let headers = signed_fetch_headers(wallet, &self.method, &self.path);
        self.headers.extend(headers);
        self
    }

    pub fn signed_meta(mut self, wallet: &Wallet, metadata: &Value) -> Self {
        let metadata = serde_json::to_string(metadata).expect("metadata json");
        let headers = signed_fetch_headers_with(wallet, &self.method, &self.path, &metadata);
        self.headers.extend(headers);
        self
    }

    pub fn json(mut self, body: &Value) -> Self {
        self.body = Some((
            serde_json::to_vec(body).expect("json body"),
            "application/json".into(),
        ));
        self
    }

    pub fn body(mut self, body: Vec<u8>, content_type: &str) -> Self {
        self.body = Some((body, content_type.into()));
        self
    }

    pub fn expect(mut self, status: u16) -> Self {
        self.expect = status;
        self
    }
}

pub struct Gate {
    spec: Value,
    coverage: BTreeMap<(String, String), Vec<u16>>,
    error_waivers: BTreeMap<(String, String), String>,
    success_waivers: BTreeMap<(String, String), String>,
}

impl Gate {
    pub fn new(spec: Value) -> Self {
        Self {
            spec,
            coverage: BTreeMap::new(),
            error_waivers: BTreeMap::new(),
            success_waivers: BTreeMap::new(),
        }
    }

    pub fn waive_error(&mut self, method: &str, spec_path: &str, reason: &str) {
        self.operation(method, spec_path);
        self.error_waivers
            .insert((method.to_uppercase(), spec_path.into()), reason.into());
    }

    pub fn waive_success(&mut self, method: &str, spec_path: &str, reason: &str) {
        self.operation(method, spec_path);
        self.success_waivers
            .insert((method.to_uppercase(), spec_path.into()), reason.into());
    }

    fn operation(&self, method: &str, spec_path: &str) -> &Value {
        let op = &self.spec["paths"][spec_path][method.to_lowercase()];
        assert!(
            op.is_object(),
            "no spec operation for {} {}",
            method,
            spec_path
        );
        op
    }

    pub async fn hit(&mut self, app: &Router, case: Case) -> Value {
        let op = self.operation(&case.method, &case.spec_path).clone();
        let ctx = format!("{} {} (as {})", case.method, case.spec_path, case.path);

        let uri = match &case.query {
            Some(q) => format!("{}?{}", case.path, q),
            None => case.path.clone(),
        };
        let mut builder = Request::builder().method(case.method.as_str()).uri(&uri);
        for (name, value) in &case.headers {
            builder = builder.header(name, value);
        }
        let body = match &case.body {
            Some((bytes, content_type)) => {
                builder = builder.header("content-type", content_type);
                Body::from(bytes.clone())
            }
            None => Body::empty(),
        };
        let request = builder.body(body).expect("request");
        let response = app.clone().oneshot(request).await.expect("infallible");

        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT)
            .await
            .expect("response body");
        let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]).to_string();

        assert_eq!(
            status, case.expect,
            "{}: expected {}, got {}\nbody: {}",
            ctx, case.expect, status, preview
        );
        let documented: Vec<String> = op["responses"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let response_spec = op["responses"].get(status.to_string()).unwrap_or_else(|| {
            panic!(
                "{}: status {} not documented (spec has {:?})\nbody: {}",
                ctx, status, documented, preview
            )
        });

        let value = match response_spec.get("content").and_then(|c| c.as_object()) {
            Some(content) => {
                let media = content.iter().find(|(declared, _)| {
                    content_type
                        .to_ascii_lowercase()
                        .starts_with(&declared.to_ascii_lowercase())
                });
                let (declared, media) = media.unwrap_or_else(|| {
                    panic!(
                        "{}: content-type {:?} not declared (spec has {:?})\nbody: {}",
                        ctx,
                        content_type,
                        content.keys().collect::<Vec<_>>(),
                        preview
                    )
                });
                if declared == "application/json" && case.method != "HEAD" {
                    let instance: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                        panic!("{}: invalid json body ({})\nbody: {}", ctx, e, preview)
                    });
                    if let Some(schema) = media.get("schema") {
                        self.validate(schema, &instance, &format!("{} -> {}", ctx, status));
                    }
                    instance
                } else {
                    Value::Null
                }
            }
            None => {
                if case.method != "HEAD" {
                    assert!(
                        bytes.is_empty(),
                        "{}: status {} declares no content but body is {}",
                        ctx,
                        status,
                        preview
                    );
                }
                Value::Null
            }
        };

        self.coverage
            .entry((case.method.clone(), case.spec_path.clone()))
            .or_default()
            .push(status);
        value
    }

    fn validate(&self, schema: &Value, instance: &Value, ctx: &str) {
        let mut doc = schema.clone();
        if let Value::Object(map) = &mut doc {
            if let Some(components) = self.spec.get("components") {
                map.insert("components".into(), components.clone());
            }
        }
        let validator = jsonschema::validator_for(&doc)
            .unwrap_or_else(|e| panic!("{}: schema does not compile: {}", ctx, e));
        let errors: Vec<String> = validator
            .iter_errors(instance)
            .map(|e| format!("  {} (at instance path {})", e, e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{}: response violates spec schema:\n{}\ninstance: {}",
            ctx,
            errors.join("\n"),
            instance
        );
    }

    pub fn assert_covered(&self) {
        let mut gaps = Vec::new();
        let paths = self.spec["paths"].as_object().expect("spec paths");
        for (path, item) in paths {
            let ops = item.as_object().expect("path item");
            for (method, op) in ops {
                if !METHODS.contains(&method.as_str()) {
                    continue;
                }
                let statuses: Vec<u16> = op["responses"]
                    .as_object()
                    .map(|m| m.keys().filter_map(|k| k.parse().ok()).collect())
                    .unwrap_or_default();
                let key = (method.to_uppercase(), path.clone());
                let hits = self.coverage.get(&key).cloned().unwrap_or_default();
                if hits.is_empty() {
                    gaps.push(format!("{} {}: never hit", method.to_uppercase(), path));
                    continue;
                }
                if statuses.iter().any(|s| *s < 400)
                    && !hits.iter().any(|s| *s < 400)
                    && !self.success_waivers.contains_key(&key)
                {
                    gaps.push(format!(
                        "{} {}: no success case exercised",
                        method.to_uppercase(),
                        path
                    ));
                }
                if statuses.iter().any(|s| (400..500).contains(s))
                    && !hits.iter().any(|s| *s >= 400)
                    && !self.error_waivers.contains_key(&key)
                {
                    gaps.push(format!(
                        "{} {}: no error case exercised",
                        method.to_uppercase(),
                        path
                    ));
                }
            }
        }
        assert!(
            gaps.is_empty(),
            "contract coverage gaps ({}):\n{}",
            gaps.len(),
            gaps.join("\n")
        );
    }
}

#[cfg(test)]
mod spec_guard_tests {
    use super::assert_usable_spec;
    use serde_json::json;

    fn refuses(spec: serde_json::Value) -> String {
        let outcome = std::panic::catch_unwind(|| assert_usable_spec("under-test", &spec));
        let payload = outcome.expect_err("assert_usable_spec accepted an unusable spec");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    #[test]
    fn a_spec_with_no_paths_is_refused() {
        let msg = refuses(json!({"openapi": "3.1.0", "paths": {}}));
        assert!(msg.contains("zero paths"), "{msg}");
    }

    #[test]
    fn a_spec_that_is_not_openapi_is_refused() {
        let msg = refuses(json!({"paths": {"/x": {"get": {"responses": {"200": {}}}}}}));
        assert!(msg.contains("openapi"), "{msg}");
    }

    #[test]
    fn an_operation_without_responses_is_refused() {
        let msg = refuses(json!({"openapi": "3.1.0", "paths": {"/x": {"get": {}}}}));
        assert!(msg.contains("declares no responses"), "{msg}");
    }

    #[test]
    fn a_path_with_no_method_is_refused() {
        let msg = refuses(json!({"openapi": "3.1.0", "paths": {"/x": {}}}));
        assert!(msg.contains("declares no HTTP method"), "{msg}");
    }

    #[test]
    fn a_real_shaped_spec_is_accepted() {
        assert_usable_spec(
            "under-test",
            &json!({"openapi": "3.1.0", "paths": {"/x": {"get": {"responses": {"200": {"description": "ok"}}}}}}),
        );
    }
}
