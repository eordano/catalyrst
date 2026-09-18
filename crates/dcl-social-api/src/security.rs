use axum::http::{HeaderMap, HeaderValue};

// Images/fonts/API calls are same-origin. Foundation RPC and LiveKit are the
// only browser network exceptions. React uses inline styles for maps/previews.
// The chat SDK uses WebAssembly; JavaScript eval and inline scripts stay blocked.
const CSP: &str = "default-src 'none'; base-uri 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' wss://rpc-social-service-ea.decentraland.org https://*.livekit.cloud wss://*.livekit.cloud; media-src 'self' blob:; worker-src 'self' blob:; object-src 'none'; frame-ancestors 'self'; form-action 'self'";

pub fn headers(headers: &mut HeaderMap) {
    for (name, value) in [
        ("content-security-policy", CSP),
        ("permissions-policy", "camera=(), microphone=(self), speaker-selection=(self), geolocation=(), display-capture=(), payment=(), usb=()"),
        // Wallet popups must retain their opener. No COEP: wallet integrations
        // and remote voice media do not require cross-origin isolation.
        ("cross-origin-opener-policy", "same-origin-allow-popups"),
        ("cross-origin-resource-policy", "same-origin"),
        ("strict-transport-security", "max-age=31536000"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    // The TLS proxy owns nosniff, Referrer-Policy and X-Frame-Options, including
    // its own 413/429 responses. Do not duplicate those headers here.
}
