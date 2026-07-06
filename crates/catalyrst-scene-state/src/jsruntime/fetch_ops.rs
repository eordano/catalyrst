use super::fetch::{sanitize_scene_headers, validate_scene_url, FetchJob, FetchResponse};
use super::scene_thread::{set_prop, str, HostState};

struct SignedFetchArgs {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

fn value_to_string(scope: &mut v8::PinScope, v: v8::Local<v8::Value>) -> Option<String> {
    if !v.is_string() {
        return None;
    }
    v.to_string(scope).map(|s| s.to_rust_string_lossy(scope))
}

fn parse_signed_fetch_args(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
) -> Result<SignedFetchArgs, &'static str> {
    let obj = v8::Local::<v8::Object>::try_from(args.get(0))
        .map_err(|_| "signedFetch expects an object argument")?;
    let url_key = str(scope, "url").into();
    let url = obj
        .get(scope, url_key)
        .and_then(|v| value_to_string(scope, v))
        .ok_or("missing url")?;

    let mut method = "GET".to_string();
    let mut headers = Vec::new();
    let mut body = None;
    let init_key = str(scope, "init").into();
    if let Some(init) = obj.get(scope, init_key) {
        if let Ok(init) = v8::Local::<v8::Object>::try_from(init) {
            let method_key = str(scope, "method").into();
            if let Some(m) = init
                .get(scope, method_key)
                .and_then(|v| value_to_string(scope, v))
            {
                method = m.to_ascii_uppercase();
            }
            let body_key = str(scope, "body").into();
            if let Some(b) = init
                .get(scope, body_key)
                .and_then(|v| value_to_string(scope, v))
            {
                body = Some(b.into_bytes());
            }
            let headers_key = str(scope, "headers").into();
            if let Some(hv) = init.get(scope, headers_key) {
                if let Ok(h) = v8::Local::<v8::Object>::try_from(hv) {
                    if let Some(names) =
                        h.get_own_property_names(scope, v8::GetPropertyNamesArgs::default())
                    {
                        for i in 0..names.length() {
                            let Some(name_val) = names.get_index(scope, i) else {
                                continue;
                            };
                            let Some(name) = name_val
                                .to_string(scope)
                                .map(|s| s.to_rust_string_lossy(scope))
                            else {
                                continue;
                            };
                            let Some(value) = h
                                .get(scope, name_val)
                                .and_then(|v| value_to_string(scope, v))
                            else {
                                continue;
                            };
                            headers.push((name, value));
                        }
                    }
                }
            }
        }
    }
    Ok(SignedFetchArgs {
        url,
        method,
        headers,
        body,
    })
}

fn fetch_error(status: u16, message: &str) -> FetchResponse {
    FetchResponse {
        status,
        status_text: message.to_string(),
        headers: Vec::new(),
        body: serde_json::json!({ "error": message }).to_string(),
    }
}

fn resolve_fetch(
    scope: &mut v8::PinScope,
    resolver: v8::Local<v8::PromiseResolver>,
    resp: &FetchResponse,
) {
    let obj = v8::Object::new(scope);
    let ok = v8::Boolean::new(scope, (200..300).contains(&resp.status));
    set_prop(scope, obj, "ok", ok.into());
    let status = v8::Number::new(scope, resp.status as f64);
    set_prop(scope, obj, "status", status.into());
    let status_text = str(scope, &resp.status_text).into();
    set_prop(scope, obj, "statusText", status_text);
    let headers = v8::Object::new(scope);
    for (name, value) in &resp.headers {
        let v = str(scope, value).into();
        set_prop(scope, headers, name, v);
    }
    set_prop(scope, obj, "headers", headers.into());
    let body = str(scope, &resp.body).into();
    set_prop(scope, obj, "body", body);
    resolver.resolve(scope, obj.into());
}

const SIGNED_FETCH_METHODS: [&str; 6] = ["GET", "POST", "PUT", "DELETE", "HEAD", "PATCH"];

// The scene only ever sees fetch RESULTS. Key material, the scope header, and
// the signed identity headers never cross into the isolate (getHeaders stays
// inert for the same reason: they would be an exfiltratable ~60s-replayable
// authoritative credential).
pub(super) fn op_signed_fetch(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(resolver) = v8::PromiseResolver::new(scope) else {
        return;
    };
    let promise = resolver.get_promise(scope);
    rv.set(promise.into());

    let req = match parse_signed_fetch_args(scope, &args) {
        Ok(r) => r,
        Err(msg) => {
            resolve_fetch(scope, resolver, &fetch_error(400, msg));
            return;
        }
    };

    let checked = HostState::with(scope, |c| {
        let mut h = c.borrow_mut();
        if h.storage.is_none() || h.fetch_tx.is_none() {
            return Err("storage is not configured");
        }
        if h.pending_fetches.len() >= h.fetch_in_flight_max {
            return Err("too many concurrent requests");
        }
        if !SIGNED_FETCH_METHODS.contains(&req.method.as_str()) {
            return Err("method not allowed");
        }
        if req.body.as_ref().map(|b| b.len()).unwrap_or(0) > h.fetch_max_body_bytes {
            return Err("request body too large");
        }
        let url = validate_scene_url(&req.url, h.storage.as_ref().unwrap())?;
        let id = h.next_fetch_id;
        h.next_fetch_id += 1;
        Ok((id, url))
    });
    let (id, url) = match checked {
        Ok(v) => v,
        Err(msg) => {
            resolve_fetch(scope, resolver, &fetch_error(400, msg));
            return;
        }
    };

    let job = FetchJob {
        id,
        method: req.method,
        url,
        body: req.body,
        headers: sanitize_scene_headers(req.headers),
    };
    let global = v8::Global::new(scope, resolver);
    let failed = HostState::with(scope, |c| {
        let mut h = c.borrow_mut();
        h.pending_fetches.insert(id, global);
        let sent = h
            .fetch_tx
            .as_ref()
            .map(|tx| tx.send(job).is_ok())
            .unwrap_or(false);
        if sent {
            None
        } else {
            h.pending_fetches.remove(&id)
        }
    });
    if let Some(g) = failed {
        let resolver = v8::Local::new(scope, g);
        resolve_fetch(
            scope,
            resolver,
            &fetch_error(500, "storage worker unavailable"),
        );
    }
}

pub(super) fn deliver_fetch_results(scope: &mut v8::PinScope) {
    let mut delivered = false;
    loop {
        let next = HostState::with(scope, |c| {
            let h = c.borrow();
            h.fetch_results.as_ref().and_then(|rx| rx.try_recv().ok())
        });
        let Some(result) = next else {
            break;
        };
        let resolver =
            HostState::with(scope, |c| c.borrow_mut().pending_fetches.remove(&result.id));
        let Some(g) = resolver else {
            continue;
        };
        let resolver = v8::Local::new(scope, g);
        let resp = match result.outcome {
            Ok(resp) => resp,
            Err(msg) => fetch_error(500, &msg),
        };
        resolve_fetch(scope, resolver, &resp);
        delivered = true;
    }
    if delivered {
        v8::tc_scope!(let tc, scope);
        tc.perform_microtask_checkpoint();
    }
}
