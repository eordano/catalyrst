//! C ABI over [`crate::host::Host`], for P/Invoke from .NET. Mirrors the ENet model:
//! `wt_host_create` -> repeatedly `wt_host_service` (poll/drain) -> `wt_peer_send_*` ->
//! `wt_host_destroy`. The host is driven from a single dedicated .NET thread.

use std::ffi::{c_char, CStr};
use std::net::SocketAddr;
use std::time::Duration;

use crate::client::{Client, ClientConfig, ClientEvent};
use crate::host::{Event, Host, HostConfig};

/// [`WtEvent::kind`] values.
pub const WT_EVENT_NONE: u32 = 0;
pub const WT_EVENT_CONNECT: u32 = 1;
pub const WT_EVENT_DISCONNECT: u32 = 2;
pub const WT_EVENT_STREAM_DATA: u32 = 3;
pub const WT_EVENT_DATAGRAM: u32 = 4;

/// Configuration for [`wt_host_create`]. All string fields are null-terminated UTF-8.
#[repr(C)]
pub struct WtConfig {
    /// Bind address as `ip:port`, e.g. `"0.0.0.0:4433"`.
    pub bind_addr: *const c_char,
    /// PEM-encoded certificate chain.
    pub cert_pem: *const c_char,
    /// PEM-encoded private key.
    pub key_pem: *const c_char,
}

/// One drained event. `data_ptr`/`data_len` are borrowed from the host and remain valid
/// only until the next `wt_*_service` or `wt_*_destroy` call on the same handle - copy out
/// immediately.
///
/// - `Connect`: `data` is the remote address as UTF-8 (`ip:port`), `reason` unused.
/// - `Disconnect`: `data` empty, `reason` carries the close code.
/// - `StreamData` / `Datagram`: `data` is the raw payload, `reason` unused.
#[repr(C)]
pub struct WtEvent {
    pub kind: u32,
    pub reason: u32,
    pub peer_id: u64,
    pub data_ptr: *const u8,
    pub data_len: usize,
}

/// Opaque host handle. Owns the [`Host`] plus a scratch buffer that keeps the most recent
/// event's payload alive for the duration documented on [`WtEvent`].
pub struct WtHost {
    host: Host,
    scratch: Vec<u8>,
}

/// # Safety
/// `config` must point to a valid [`WtConfig`] whose string fields are valid null-terminated
/// UTF-8. Returns a non-null handle on success, or null on failure (bad config / bind / cert).
#[no_mangle]
pub unsafe extern "C" fn wt_host_create(config: *const WtConfig) -> *mut WtHost {
    if config.is_null() {
        return std::ptr::null_mut();
    }
    let config = &*config;
    let (Some(bind_addr), Some(cert_pem), Some(key_pem)) = (
        cstr_to_str(config.bind_addr),
        cstr_to_str(config.cert_pem),
        cstr_to_str(config.key_pem),
    ) else {
        return std::ptr::null_mut();
    };
    let Ok(bind_addr) = bind_addr.parse::<SocketAddr>() else {
        return std::ptr::null_mut();
    };

    match Host::new(HostConfig {
        bind_addr,
        cert_pem: cert_pem.to_owned(),
        key_pem: key_pem.to_owned(),
    }) {
        Ok(host) => Box::into_raw(Box::new(WtHost {
            host,
            scratch: Vec::new(),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `host` must be a handle from [`wt_host_create`], not used after this call. Null is ignored.
#[no_mangle]
pub unsafe extern "C" fn wt_host_destroy(host: *mut WtHost) {
    if !host.is_null() {
        drop(Box::from_raw(host));
    }
}

/// Block up to `timeout_ms` for the next event. Writes it to `out_event` and returns 1, or
/// returns 0 if nothing arrived. `out_event.data_ptr` is valid until the next call.
///
/// # Safety
/// `host` must be a valid handle and `out_event` a valid, writable [`WtEvent`].
#[no_mangle]
pub unsafe extern "C" fn wt_host_service(
    host: *mut WtHost,
    timeout_ms: u32,
    out_event: *mut WtEvent,
) -> i32 {
    if host.is_null() || out_event.is_null() {
        return 0;
    }
    let host = &mut *host;
    let Some(event) = host
        .host
        .service(Duration::from_millis(u64::from(timeout_ms)))
    else {
        return 0;
    };

    let (kind, reason, peer_id, payload): (u32, u32, u64, Vec<u8>) = match event {
        Event::Connect {
            peer_id,
            remote_addr,
        } => (WT_EVENT_CONNECT, 0, peer_id, remote_addr.into_bytes()),
        Event::Disconnect { peer_id, reason } => (WT_EVENT_DISCONNECT, reason, peer_id, Vec::new()),
        Event::StreamData { peer_id, data } => (WT_EVENT_STREAM_DATA, 0, peer_id, data.to_vec()),
        Event::Datagram { peer_id, data } => (WT_EVENT_DATAGRAM, 0, peer_id, data.to_vec()),
    };

    host.scratch = payload;
    let out = &mut *out_event;
    out.kind = kind;
    out.reason = reason;
    out.peer_id = peer_id;
    out.data_ptr = host.scratch.as_ptr();
    out.data_len = host.scratch.len();
    1
}

/// Queue raw bytes for the peer's reliable bidi stream. Returns 1 on success, 0 if the peer
/// is gone.
///
/// # Safety
/// `host` must be valid; `data` must point to `len` readable bytes (may be null iff `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn wt_peer_send_stream(
    host: *mut WtHost,
    peer_id: u64,
    data: *const u8,
    len: usize,
) -> i32 {
    if host.is_null() || (data.is_null() && len != 0) {
        return 0;
    }
    let host = &*host;
    let slice: &[u8] = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data, len)
    };
    i32::from(host.host.send_stream(peer_id, slice))
}

/// Send an unreliable datagram to the peer. Returns 1 on success, 0 on failure.
///
/// # Safety
/// `host` must be valid; `data` must point to `len` readable bytes (may be null iff `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn wt_peer_send_datagram(
    host: *mut WtHost,
    peer_id: u64,
    data: *const u8,
    len: usize,
) -> i32 {
    if host.is_null() || (data.is_null() && len != 0) {
        return 0;
    }
    let host = &*host;
    let slice: &[u8] = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data, len)
    };
    i32::from(host.host.send_datagram(peer_id, slice))
}

/// Close the peer's session with an application error code. Returns 1 on success, 0 if gone.
///
/// # Safety
/// `host` must be a valid handle.
#[no_mangle]
pub unsafe extern "C" fn wt_peer_disconnect(host: *mut WtHost, peer_id: u64, reason: u32) -> i32 {
    if host.is_null() {
        return 0;
    }
    let host = &*host;
    i32::from(host.host.disconnect(peer_id, reason))
}

/// Write the peer's smoothed RTT (microseconds) to `out_rtt_us`. Returns 1 if connected, else 0.
///
/// # Safety
/// `host` must be valid and `out_rtt_us` a valid, writable `u64`.
#[no_mangle]
pub unsafe extern "C" fn wt_peer_rtt_us(
    host: *mut WtHost,
    peer_id: u64,
    out_rtt_us: *mut u64,
) -> i32 {
    if host.is_null() || out_rtt_us.is_null() {
        return 0;
    }
    let host = &*host;
    match host.host.rtt_us(peer_id) {
        Some(rtt) => {
            *out_rtt_us = rtt;
            1
        }
        None => 0,
    }
}

// ---------- Client ----------

/// Opaque client handle returned by [`wt_client_connect`]. Owns the [`Client`] plus a scratch
/// buffer (same event-payload lifetime contract as [`WtHost`]).
pub struct WtClient {
    client: Client,
    scratch: Vec<u8>,
}

/// Connect to a WebTransport server. `url` is null-terminated UTF-8 (e.g. `"https://host:port/"`).
/// If `cert_hash_len` is 32, `cert_hash` is the SHA-256 of the server certificate to trust (the
/// self-signed dev path); otherwise the system root store is used. Returns null on failure.
///
/// # Safety
/// `url` must be valid null-terminated UTF-8. `cert_hash` must point to `cert_hash_len` readable
/// bytes (may be null iff `cert_hash_len` is 0).
#[no_mangle]
pub unsafe extern "C" fn wt_client_connect(
    url: *const c_char,
    cert_hash: *const u8,
    cert_hash_len: usize,
) -> *mut WtClient {
    let Some(url) = cstr_to_str(url) else {
        return std::ptr::null_mut();
    };
    let server_cert_hash = if !cert_hash.is_null() && cert_hash_len > 0 {
        Some(std::slice::from_raw_parts(cert_hash, cert_hash_len).to_vec())
    } else {
        None
    };

    match Client::connect(ClientConfig {
        url: url.to_owned(),
        server_cert_hash,
    }) {
        Ok(client) => Box::into_raw(Box::new(WtClient {
            client,
            scratch: Vec::new(),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `client` must be a handle from [`wt_client_connect`], not used after this call. Null is ignored.
#[no_mangle]
pub unsafe extern "C" fn wt_client_destroy(client: *mut WtClient) {
    if !client.is_null() {
        drop(Box::from_raw(client));
    }
}

/// Drain one client event (same data-lifetime contract as [`wt_host_service`]). `peer_id` is
/// always 0 - a client has a single session.
///
/// # Safety
/// `client` must be valid and `out_event` a valid, writable [`WtEvent`].
#[no_mangle]
pub unsafe extern "C" fn wt_client_service(
    client: *mut WtClient,
    timeout_ms: u32,
    out_event: *mut WtEvent,
) -> i32 {
    if client.is_null() || out_event.is_null() {
        return 0;
    }
    let client = &mut *client;
    let Some(event) = client
        .client
        .service(Duration::from_millis(u64::from(timeout_ms)))
    else {
        return 0;
    };

    let (kind, reason, payload): (u32, u32, Vec<u8>) = match event {
        ClientEvent::Disconnected { reason } => (WT_EVENT_DISCONNECT, reason, Vec::new()),
        ClientEvent::StreamData { data } => (WT_EVENT_STREAM_DATA, 0, data.to_vec()),
        ClientEvent::Datagram { data } => (WT_EVENT_DATAGRAM, 0, data.to_vec()),
    };

    client.scratch = payload;
    let out = &mut *out_event;
    out.kind = kind;
    out.reason = reason;
    out.peer_id = 0;
    out.data_ptr = client.scratch.as_ptr();
    out.data_len = client.scratch.len();
    1
}

/// Queue raw bytes for the reliable bidi stream. Returns 1 on success, 0 if the session is gone.
///
/// # Safety
/// `client` must be valid; `data` must point to `len` readable bytes (may be null iff `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn wt_client_send_stream(
    client: *mut WtClient,
    data: *const u8,
    len: usize,
) -> i32 {
    if client.is_null() || (data.is_null() && len != 0) {
        return 0;
    }
    let client = &*client;
    let slice: &[u8] = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data, len)
    };
    i32::from(client.client.send_stream(slice))
}

/// Send an unreliable datagram. Returns 1 on success, 0 on failure.
///
/// # Safety
/// `client` must be valid; `data` must point to `len` readable bytes (may be null iff `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn wt_client_send_datagram(
    client: *mut WtClient,
    data: *const u8,
    len: usize,
) -> i32 {
    if client.is_null() || (data.is_null() && len != 0) {
        return 0;
    }
    let client = &*client;
    let slice: &[u8] = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data, len)
    };
    i32::from(client.client.send_datagram(slice))
}

/// Close the session with an application error code. Returns 1.
///
/// # Safety
/// `client` must be a valid handle.
#[no_mangle]
pub unsafe extern "C" fn wt_client_disconnect(client: *mut WtClient, reason: u32) -> i32 {
    if client.is_null() {
        return 0;
    }
    let client = &*client;
    client.client.disconnect(reason);
    1
}

/// Write the smoothed RTT to the server (microseconds) to `out_rtt_us`. Returns 1.
///
/// # Safety
/// `client` must be valid and `out_rtt_us` a valid, writable `u64`.
#[no_mangle]
pub unsafe extern "C" fn wt_client_rtt_us(client: *mut WtClient, out_rtt_us: *mut u64) -> i32 {
    if client.is_null() || out_rtt_us.is_null() {
        return 0;
    }
    let client = &*client;
    *out_rtt_us = client.client.rtt_us();
    1
}

/// Convert a C string pointer to a `&str`, or `None` if null or not valid UTF-8.
///
/// # Safety
/// `ptr` must be null or a valid null-terminated C string living at least as long as `'a`.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}
