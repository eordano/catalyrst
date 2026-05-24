"""Custom schemathesis checks for catalyrst.

Loaded by run.sh via SCHEMATHESIS_HOOKS=checks. Each @schemathesis.check
function receives (response, case) and must raise AssertionError to fail.
The default `response_schema_conformance` check is re-registered here as a
no-op alias so we have a single, explicit place to enumerate what we enforce.
"""
from __future__ import annotations

import json
from typing import Any

import schemathesis
from schemathesis.checks import (
    response_schema_conformance as _default_response_schema_conformance,
)


@schemathesis.check
def not_a_server_error(response: Any, case: Any) -> None:
    """Fail on any 5xx response that isn't explicitly documented in the spec.

    schemathesis ships a similar default, but we register it explicitly so the
    intent is visible alongside our custom checks and so it's always on even
    when callers narrow --checks.
    """
    status = getattr(response, "status_code", None)
    if status is None or status < 500 or status >= 600:
        return

    documented = set()
    operation = getattr(case, "operation", None)
    if operation is not None:
        responses = getattr(operation, "definition", {}) or {}
        if isinstance(responses, dict):
            documented = set((responses.get("responses") or {}).keys())

    if str(status) in documented or "default" in documented:
        return

    raise AssertionError(
        f"undocumented server error: {status} for "
        f"{getattr(case, 'method', '?')} {getattr(case, 'path', '?')}"
    )


@schemathesis.check
def response_schema_conformance(response: Any, case: Any) -> None:
    """Re-export the built-in response/schema conformance check.

    Delegates to schemathesis' default implementation. Listed here so the
    set of enforced checks is discoverable from one file.
    """
    _default_response_schema_conformance(response, case)


@schemathesis.check
def cors_headers_present(response: Any, case: Any) -> None:
    """If the request had an Origin header, the response must echo
    Access-Control-Allow-Origin (either the origin or '*').
    """
    headers = getattr(case, "headers", None) or {}
    # case.headers may be case-insensitive depending on the schemathesis
    # version; normalise lookup.
    origin = None
    for k, v in headers.items():
        if k.lower() == "origin":
            origin = v
            break
    if origin is None:
        return

    resp_headers = getattr(response, "headers", {}) or {}
    allow_origin = None
    for k, v in resp_headers.items():
        if k.lower() == "access-control-allow-origin":
            allow_origin = v
            break

    if allow_origin is None:
        raise AssertionError(
            "request sent with Origin header but response is missing "
            "Access-Control-Allow-Origin"
        )

    if allow_origin != "*" and allow_origin != origin:
        raise AssertionError(
            f"Access-Control-Allow-Origin '{allow_origin}' does not echo "
            f"request Origin '{origin}' (and is not '*')"
        )


@schemathesis.check
def error_body_shape(response: Any, case: Any) -> None:
    """Non-2xx responses must have body `{"error": str, "message"?: str}`
    per the catalyrst error contract. Empty bodies (e.g. 204, HEAD) are
    skipped, as are non-JSON content types.
    """
    status = getattr(response, "status_code", None)
    if status is None or 200 <= status < 300:
        return

    # No body / non-JSON content -> nothing to enforce here.
    content_type = ""
    resp_headers = getattr(response, "headers", {}) or {}
    for k, v in resp_headers.items():
        if k.lower() == "content-type":
            content_type = v or ""
            break
    if "json" not in content_type.lower():
        return

    raw = getattr(response, "content", None)
    if raw is None:
        raw = getattr(response, "text", None)
    if raw is None or (hasattr(raw, "__len__") and len(raw) == 0):
        return

    try:
        if isinstance(raw, (bytes, bytearray)):
            body = json.loads(raw.decode("utf-8", errors="replace"))
        elif isinstance(raw, str):
            body = json.loads(raw)
        else:
            body = raw
    except (ValueError, UnicodeDecodeError) as exc:
        raise AssertionError(
            f"non-2xx response ({status}) advertised JSON but body did not "
            f"parse: {exc}"
        )

    if not isinstance(body, dict):
        raise AssertionError(
            f"non-2xx response ({status}) JSON body is not an object: "
            f"{type(body).__name__}"
        )

    if "error" not in body:
        raise AssertionError(
            f"non-2xx response ({status}) missing required 'error' field; "
            f"got keys: {sorted(body.keys())}"
        )

    if not isinstance(body["error"], str):
        raise AssertionError(
            f"non-2xx response ({status}) 'error' field must be a string, "
            f"got {type(body['error']).__name__}"
        )

    if "message" in body and not isinstance(body["message"], str):
        raise AssertionError(
            f"non-2xx response ({status}) 'message' field, when present, "
            f"must be a string, got {type(body['message']).__name__}"
        )
