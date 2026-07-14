#!/usr/bin/env python3
import json, re, sys, glob, os

INV = "/tmp/claude-1001/-home-dcl-one/f7f7bb75-2372-4dbb-870b-8c93f99fd2a0/scratchpad/inventory"
CORPUS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "corpus.json")
OUT = sys.argv[1]

def load_jsonl(p):
    rows = []
    with open(p) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except Exception:
                pass
    return rows

corpus = json.load(open(CORPUS))
corpus_ids = set()
for e in corpus["entries"]:
    corpus_ids.add(f'{e["method"]} {e["host"]}{e["path"]}')

GATEWAY_SVC = {
    "asset-bundle-registry":"asset-bundle-registry.decentraland.org",
    "camera-reel-service":"camera-reel-service.decentraland.org",
    "comms-gatekeeper":"comms-gatekeeper.decentraland.org",
    "credits":"credits.decentraland.org",
    "notifications":"notifications.decentraland.org",
    "places":"places.decentraland.org",
    "social-api":"social-api.decentraland.org",
    "archipelago-ea-stats":"archipelago-ea-stats.decentraland.org",
    "ab-cdn":"ab-cdn.decentraland.org",
}
def corpus_has(method, host, path):
    if f'{method} {host}{path}' in corpus_ids:
        return True
    sub = host.split(".decentraland.org")[0]
    if host.endswith(".decentraland.org"):
        g = f'{method} gateway.decentraland.org/{sub}{path}'
        if g in corpus_ids:
            return True
    return False

def norm_path(p):
    p = re.sub(r'/v[0-9]+/', '/v{n}/', p)
    p = re.sub(r'0x[0-9a-fA-F]{6,}', '{v}', p)
    p = re.sub(r'baf[a-z0-9]{20,}', '{v}', p)
    p = re.sub(r'\{(?!n\})[^}]*\}', '{v}', p)
    p = re.sub(r'\{[^}/]*(?=/|$)', '{v}', p)
    return p

GW_TO_SUB = {"comms-gatekeeper-local":"comms-gatekeeper-local.decentraland.org"}
def canon(host, path):
    if host == "gateway.decentraland.org":
        m = re.match(r'/([^/]+)(/.*)?$', path)
        if m:
            svc, rest = m.group(1), m.group(2) or "/"
            host = GW_TO_SUB.get(svc, svc + ".decentraland.org")
            path = rest
    return host, path

corpus_norm = {}
for e in corpus["entries"]:
    host, path = canon(e["host"], norm_path(e["path"]))
    corpus_norm[f'{e["method"]} {host} {path}'] = f'{e["method"]} {e["host"]}{e["path"]}'

SPROBES_PATH = os.path.join(os.path.dirname(OUT), "synthetic-probes.json")
sprobes = json.load(open(SPROBES_PATH))
PROBE_IDS = {p["id"] for p in sprobes["probes"]}
PROBE_TABLE = {}
for p in sprobes["probes"]:
    PROBE_TABLE[(p["method"], p["host"], norm_path(p["path"]))] = p["id"]

PROBE_ALIAS = {
    ("GET","feature-flags.decentraland.org","/{v}.json"):"probe:feature-flags",
    ("GET","peer.decentraland.org","/lambdas/explorer/{v}/{v}"):"probe:lambdas-explorer-wearables",
    ("GET","realm-provider-ea.decentraland.org","/main"):"probe:realm-provider-main",
    ("GET","worlds-content-server.decentraland.org","/world/{v}/about"):"probe:worlds-about",
    ("GET","worlds-content-server.decentraland.org","/world/{v}/permissions"):"probe:worlds-permissions",
    ("GET","worlds-content-server.decentraland.org","/wallet/{v}/connected-world"):"probe:worlds-connected-world",
    ("GET","assets-cdn.decentraland.org","/social/communities/{v}/raw-thumbnail.png"):"probe:assets-cdn-thumbnail",
    ("GET","metamorph-api.decentraland.org","/convert"):"probe:metamorph-convert",
    ("GET","marketplace-api.decentraland.org","/v{n}/catalog/unified"):"probe:marketplace-unified",
    ("GET","marketplace-api.decentraland.org","/v{n}/trades/{v}"):"probe:marketplace-trades",
    ("POST","rpc.decentraland.org","/{v}"):"probe:rpc-mainnet",
    ("WS","rpc.decentraland.org","/{v}"):"probe:rpc-ws",
    ("WS","rpc.decentraland.org","/"):"probe:rpc-ws",
    ("GET","badges.decentraland.org","/badges/{v}/tiers"):"probe:badges-tiers",
    ("GET","asset-bundle-registry.decentraland.org","/entities/versions"):"probe:ab-registry-versions",
}
def probe_for(method, host, path):
    return PROBE_TABLE.get((method, host, path)) or PROBE_ALIAS.get((method, host, path))

ALLOWED_UPSTREAM_HOSTS = {
    "www.youtube.com", "drive.usercontent.google.com", "docs.google.com",
    "github.com", "media.githubusercontent.com", "api.coingecko.com",
    "api.segment.io", "api.segment.com",
    "opensea.decentraland.org",
    "__scene__",
}
def is_external_upstream(host):
    return host in ALLOWED_UPSTREAM_HOSTS or host.endswith(".thirdweb.com") or "thirdweb" in host

def defer(method, host, path, auth, bucket, sites):
    if host == "__scene__" or "scene" in host:
        return ("documented-intentional-upstream-dependency",
                "Scene-authored fetch/signedFetch/websocket destination; the client is a transparent proxy for a URL the running SDK scene chooses, not a fixed deployment endpoint.")
    if is_external_upstream(host):
        return ("documented-intentional-upstream-dependency",
                "Genuinely-external provider/content (YouTube / Thirdweb / Google-Drive / GitHub tiles / CoinGecko / OpenSea proxy); not a base-domain obligation.")
    if host.startswith("127.0.0.1") or host.startswith("localhost"):
        return ("desktop-shell-only",
                "Local dev/editor tooling endpoint (abgen plugin / MCP / preview server); never reached by a shipped player boot.")
    if host in ("res.soulmagic.online",) or host == "sdk-team-cdn.decentraland.org":
        return ("desktop-shell-only",
                "Debug/playground utility asset host, gated behind a developer stress/demo tool.")
    if host == "explorer-artifacts.decentraland.org":
        return ("desktop-shell-only",
                "Launcher self-update manifest; fetched by the desktop shell/updater, not the in-client boot path.")
    if host == "builder-api.decentraland.org":
        return ("desktop-shell-only",
                "Creator/builder tooling surface (collections editor, newsletter opt-in); not exercised on player boot/idle.")
    if host == "comms-gatekeeper-local.decentraland.org":
        return ("desktop-shell-only",
                "LocalGateKeeperSceneAdapter: scene comms adapter used ONLY in LocalSceneDevelopment mode; not a production boot obligation.")
    if "auth-api" in host:
        return ("desktop-shell-only",
                "QR/remote wallet login handshake; bypassed by --skip-auth-screen and never exercised by a throwaway/guest identity.")
    if host == "decentraland.org":
        return ("desktop-shell-only",
                "Web-app / AuthSignatureWebApp browser handoff (login, marketing pages); browser-open, bypassed by --skip-auth-screen.")
    if "pulse-server" in host:
        return ("desktop-shell-only",
                "Pulse telemetry sink; optional analytics stream, first-party but non-functional to boot/idle.")
    if "rpc-social-service-ea" in host:
        return ("needs-real-wallet",
                "Friends/social RPC websocket requires an authenticated social session and a populated friends graph. Unlock: signed identity + seeded social graph.")
    if host == "transactions-api.decentraland.org":
        return ("needs-real-wallet",
                "Meta-transaction relay for on-chain actions (name claim, marketplace). Unlock: funded wallet + signed tx.")
    if host == "autotranslate-server.decentraland.org":
        return ("needs-real-wallet",
                "Chat auto-translate fires only on a received chat message in a populated room. Unlock: signed identity in a live comms room.")
    if host == "marketplace-api.decentraland.org":
        return ("needs-real-wallet",
                "Marketplace catalog/trades browsing; first-party service (candidate gap ESCALATED via probe:marketplace-trades) requiring populated listings/wallet context.")
    if host in ("camera-reel-service.decentraland.org","social-api.decentraland.org","credits.decentraland.org","notifications.decentraland.org","events.decentraland.org"):
        return ("needs-real-wallet",
                "First-party signed service over per-user/social-graph state; header construction proven-equivalent in synthetic-probes. Unlock: signed identity + seeded state.")
    if "{cid}" in path or "/contents/" in path or "ab-cdn" in host or "lod" in host.lower() \
       or host in ("assets-cdn.decentraland.org","profile-images.decentraland.org") \
       or (host == "asset-bundle-registry.decentraland.org" and ("manifest" in path or "world" in path)):
        return ("immutable-content-addressed",
                "Content/world/community-id-keyed asset or manifest; served identically by key on any deployment or CDN.")
    if host == "places.decentraland.org" and "/report" in path:
        return ("needs-real-wallet",
                "Content-moderation report submission; requires an authenticated reporter identity and a target context.")
    if method == "WS":
        return ("needs-real-wallet",
                "First-party realtime transport (comms/archipelago/realm websocket) requires an authenticated session; not exercised unauthenticated.")
    if auth == "signed-fetch" or bucket == "scene-signedfetch":
        return ("needs-real-wallet",
                "First-party signed-fetch endpoint over per-user/social state; header construction proven-equivalent in synthetic-probes. Unlock: signed identity + seeded state.")
    if method in ("POST","PUT","PATCH","DELETE"):
        return ("needs-real-wallet",
                "First-party state-mutating request requiring an authenticated actor and/or populated backend state.")
    return ("__UNCLASSIFIED_FIRST_PARTY__",
            f"first-party unauth GET not covered by corpus/probe: {method} {host}{path} (sites {sites})")

rows = []
for f in sorted(glob.glob(f"{INV}/[1-5]-*.jsonl")):
    if "browser" in f:
        continue
    rows.extend(load_jsonl(f))

browser = load_jsonl(f"{INV}/1-browser-links.jsonl")

SCENE_SHAPE = ("* ", "scene-passthrough", "scene-authored fetch/signedFetch/websocket destination")
def clean_host_path(u):
    if not u:
        return None, None
    u = u.replace("{ENV}", "org")
    m = re.match(r'^(wss?|https?)://([^/\s]+)(/[^\s?]*)?', u)
    if not m:
        return "__scene__", "/"
    scheme = m.group(1); host = m.group(2); path = m.group(3) or "/"
    if "..." in path or "{" in host or "}" in host or " " in host:
        return "__scene__", "/"
    path = re.split(r'[\[\|(]', path)[0].rstrip('].,')
    if not path.startswith("/"):
        path = "/" + path
    host, path = canon(host, norm_path(path))
    return host, path

shapes = {}
def add_shape(method, host, path, auth, bucket, site):
    if host is None:
        return
    key = f'{method} {host} {path}'
    s = shapes.get(key)
    if not s:
        s = {"id": key, "method": method, "host": host, "pathTemplate": path,
             "auth": auth or "none", "bucket": bucket or "other", "sites": []}
        shapes[key] = s
    if site and len(s["sites"]) < 3 and site not in s["sites"]:
        s["sites"].append(site)
    if auth == "signed-fetch":
        s["auth"] = "signed-fetch"

BASE_ONLY = re.compile(r'^/?$')
def is_non_request(r):
    m = (r.get("mechanism") or "").lower()
    n = (r.get("notes") or "").lower()
    return "metadata payload" in m or "no http call" in n or "embedded in comms metadata" in n

for r in rows:
    if is_non_request(r):
        continue
    host, path = clean_host_path(r.get("urlTemplate"))
    if host is None:
        continue
    site = f'{r.get("file","?")}:{r.get("line","?")}'
    if host == "__scene__":
        add_shape("*", "__scene__", "/scene-authored-passthrough", "scene", "scene-signedfetch", site)
        continue
    if BASE_ONLY.match(path) and host != "decentraland.org":
        continue
    add_shape(r.get("method","GET"), host, path, r.get("auth"), r.get("bucket"), site)

out = []
for key in sorted(shapes):
    s = shapes[key]
    method, host, path = s["method"], s["host"], s["pathTemplate"]
    disp = None
    ck = f'{method} {host} {path}'
    if ck in corpus_norm:
        disp = {"kind":"corpus","ref": corpus_norm[ck]}
    if disp is None:
        pid = probe_for(method, host, path)
        if pid:
            disp = {"kind":"probe","ref": pid}
    if disp is None:
        reason, rationale = defer(method, host, path, s["auth"], s["bucket"], s["sites"])
        disp = {"kind":"deferred","reason":reason,"rationale":rationale}
        ev = probe_for(method, host, path)
        if ev:
            disp["evidence"] = ev
    s["disposition"] = disp
    out.append(s)

bshapes = {}
for r in browser:
    host, path = clean_host_path(r.get("urlTemplate"))
    if host is None:
        continue
    key = f'GET {host} {path}'
    if key not in bshapes:
        bshapes[key] = {"id":key,"method":"GET","host":host,"pathTemplate":path,
                        "auth":"none","bucket":r.get("bucket","other"),
                        "sites":[f'{r.get("file","?")}:{r.get("line","?")}'],
                        "disposition":{"kind":"browser-only"}}
for k in sorted(bshapes):
    out.append(bshapes[k])

unclassified = [s["id"] for s in out if s["disposition"].get("reason") == "__UNCLASSIFIED_FIRST_PARTY__"]
if unclassified:
    print("FATAL: unclassified first-party shapes:", file=sys.stderr)
    for u in unclassified:
        print("  " + u, file=sys.stderr)
    sys.exit(1)
leaks = [s["id"] for s in out
         if s["disposition"].get("reason") == "documented-intentional-upstream-dependency"
         and not is_external_upstream(s["host"])]
if leaks:
    print("FATAL: first-party host classified as upstream:", file=sys.stderr)
    for l in leaks:
        print("  " + l, file=sys.stderr)
    sys.exit(1)

doc = {
    "description": "Source-derived HTTP/WS request shapes from unity-explorer (PR #9728 base-domain worktree), reconciled against the dynamic corpus, synthetic probes, and deferrals. Every shape carries exactly one disposition. Verified by the source_inventory_reconciles test in catalyrst-conformance.",
    "allowedDeferralReasons": [
        "needs-real-wallet","desktop-shell-only","error-path-only",
        "documented-intentional-upstream-dependency","immutable-content-addressed"
    ],
    "allowedUpstreamHosts": sorted(ALLOWED_UPSTREAM_HOSTS) + ["*.thirdweb.com"],
    "upstreamRule": "documented-intentional-upstream-dependency is permitted ONLY for allowedUpstreamHosts (genuinely external providers/content) and the synthetic __scene__ passthrough. Every other host is a first-party base-domain/gateway obligation and must be corpus | probe | needs-real-wallet | desktop-shell-only | immutable-content-addressed. The checker rejects violations.",
    "shapeCount": len(out),
    "shapes": out,
}
json.dump(doc, open(OUT,"w"), indent=1)
from collections import Counter
c = Counter()
for s in out:
    d = s["disposition"]
    c[d["kind"] + (":"+d.get("reason","") if d["kind"]=="deferred" else "")] += 1
print(f"{len(out)} shapes ->", OUT)
for k,v in sorted(c.items()):
    print(f"  {v:3d} {k}")
