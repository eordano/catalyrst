import concurrent.futures
from pathlib import Path
import ssl
import urllib.error
import urllib.request

root = Path("/var/lib/catalyrst/test-play")
assets = {
    "pkg/manifest.json": b'{"fixture":true}',
    "pkg/webgpu_build_bg.wasm": bytes([0, 97, 115, 109, 1, 0, 0, 0]),
    "assets_bundle.bin": b"assets",
    "favicon/favicon.svg": b"<svg/>",
    "ui3-overlay/fonts/Inter-Medium.woff2": b"font",
    "ui3-overlay/chunks/start.js": b"export {};",
}
for name, contents in assets.items():
    target = root / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(contents)

context = ssl._create_unverified_context()


def fetch(path):
    request = urllib.request.Request(
        "https://127.0.0.1" + path, headers={"Host": "test.local"}
    )
    try:
        with urllib.request.urlopen(request, context=context, timeout=15) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


with concurrent.futures.ThreadPoolExecutor(max_workers=32) as pool:
    responses = list(pool.map(fetch, ["/places"] * 240))
    assert any(status == 429 for status, _ in responses), "API budget was not exhausted"
    paths = list(assets) * 32
    responses = list(pool.map(fetch, ["/play/" + name for name in paths]))
    for name, (status, body) in zip(paths, responses):
        assert status == 200 and body == assets[name], (name, status, body[:80])
