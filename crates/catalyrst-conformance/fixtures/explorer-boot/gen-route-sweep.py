#!/usr/bin/env python3
import json, subprocess, re, sys, os, time

F = os.path.dirname(os.path.abspath(__file__))
ONE = os.environ.get("ONE_ROOT", os.path.abspath(os.path.join(F, "../../../../..")))
CAND = "interconnected.online"
ORIGIN_IP = os.environ.get("CANDIDATE_ORIGIN_IP") or sys.exit("set CANDIDATE_ORIGIN_IP to the candidate origin")
ADDR = "0x8d6f63e382d73cf53858864f673f39e9ff915a1e"
UUID = "00000000-0000-0000-0000-000000000000"

GATEWAY_PAIRS = {
    "places":"places","api":"api","archipelago-ea-stats":"archipelago-ea-stats",
    "worlds-content-server":"worlds-content-server","asset-bundle-registry":"asset-bundle-registry",
    "ab-cdn":"ab-cdn","camera-reel-service":"camera-reel-service","social-api":"social-api",
    "comms-gatekeeper":"comms-gatekeeper","notifications":"notifications","badges":"badges",
    "metamorph-api":"metamorph-api","assets-cdn":"assets-cdn","credits":"credits",
    "realm-provider-ea":"realm-provider-ea","auth-api":"auth-api","profile-images":"profile-images",
    "marketplace-api":"market",
    "autotranslate-server":"autotranslate-server","transactions-api":"transactions-api",
}

def fill(path):
    def sub(m):
        pre = path[:m.start()].lower()
        if any(k in pre for k in ("user","wallet","member","profile","ban","credit")):
            return ADDR
        if any(k in pre for k in ("communit","image","place","event","post","order","reel","request")):
            return UUID
        return "0"
    p = re.sub(r'\{n\}', '1', path)
    p = re.sub(r'\{[^}]*\}', sub, p)
    return p

def _curl_once(method, vhost, path):
    url = f"https://{vhost}{path}"
    hdr = ["-X", method, "--resolve", f"{vhost}:443:{ORIGIN_IP}"]
    if method == "OPTIONS":
        hdr += ["-H", "Origin: https://play.decentraland.org", "-H", "Access-Control-Request-Method: POST"]
    try:
        out = subprocess.run(
            ["curl","-sS","-o","/tmp/rs_body","-D","/tmp/rs_hdr","--max-time","15","-w","%{http_code}",*hdr,url],
            capture_output=True, text=True, timeout=25)
        status = int(out.stdout.strip() or "0")
    except Exception:
        return (0, "", "", "", url)
    ctype = server = ""
    cfray = False
    try:
        for line in open("/tmp/rs_hdr"):
            ll = line.lower()
            if ll.startswith("content-type:"):
                ctype = line.split(":",1)[1].strip().lower()
            elif ll.startswith("server:"):
                server = line.split(":",1)[1].strip().lower()
            elif ll.startswith("cf-ray:"):
                cfray = True
    except Exception:
        pass
    body = ""
    try:
        body = open("/tmp/rs_body","rb").read(120).decode("utf-8","replace")
    except Exception:
        pass
    return (status, ctype, body, server + ("+cf-ray" if cfray else ""), url)

def is_cloudflare(status, body, server):
    return ("cloudflare" in server) or ("cf-ray" in server) or bool(re.search(r'error code:\s*\d+', body.lower()))

def is_transient(status, body, server):
    return status in (0, 502, 503, 429) or is_cloudflare(status, body, server)

def probe(method, vhost, path):
    st, ct, bd, sv, url = _curl_once(method, vhost, path)
    delay = 0.8
    for _ in range(3):
        if not is_transient(st, bd, sv):
            break
        time.sleep(delay); delay = min(delay*2, 5.0)
        st, ct, bd, sv, url = _curl_once(method, vhost, path)
    if is_cloudflare(st, bd, sv):
        present = None
    elif st in (0, 502, 503):
        present = None
    elif st in (404, 405):
        b = bd.strip().lower()
        if (ct.startswith("text/plain") and (b == "" or b == "not found")) or \
           (ct.startswith("text/html") and ("404 not found" in b or "nginx" in b)):
            present = False
        else:
            present = True
    else:
        present = True
    return {"url": url, "method": method, "status": st, "server": sv,
            "contentType": ct, "body": bd.strip()[:48], "routePresent": present}

sprobes = json.load(open(f"{F}/synthetic-probes.json"))["probes"]
def norm(p):
    p = re.sub(r'/v[0-9]+/', '/v{n}/', p); p = re.sub(r'\{[^}]*\}', '{v}', p)
    return p
AUTH_EVIDENCE = {}
for p in sprobes:
    if p["instrument"] == "signed":
        AUTH_EVIDENCE[(p["method"], p["host"], norm(p["path"]))] = {"id": p["id"], "candidate": p["candidate"]}

def wait_warmup():
    for _ in range(20):
        r = _curl_once("GET", f"peer.{CAND}", "/about")
        if r[0] == 200:
            print(f"warmup ok: peer/about origin 200", flush=True); return True
        print(f"warmup: peer/about origin {r[0]} (server={r[3]}) - waiting", flush=True)
        time.sleep(15)
    print("warmup gate not satisfied after 5min; proceeding (results may show 503 unproven)", flush=True)
    return False

def main():
    wait_warmup()
    inv = json.load(open(f"{F}/source-inventory.json"))
    shapes = [s for s in inv["shapes"]
              if s["disposition"]["kind"] == "deferred"
              and s["disposition"]["reason"] == "needs-real-wallet"
              and s["host"].endswith(".decentraland.org")]
    rows = []
    for s in shapes:
        method, host, path = s["method"], s["host"], s["pathTemplate"]
        sub = host.split(".decentraland.org")[0]
        concrete = fill(path)
        pm = "GET" if method in ("GET","HEAD","WS") else "OPTIONS"
        mapping = "dual" if sub in GATEWAY_PAIRS else "subdomain-only"
        sd = probe(pm, f"{sub}.{CAND}", concrete)
        gw = probe(pm, f"gateway.{CAND}", f"/{GATEWAY_PAIRS[sub]}{concrete}") if mapping == "dual" else None
        ev = AUTH_EVIDENCE.get(("GET", host, norm(path))) if method == "GET" else None
        missing = []
        if sd["routePresent"] is not True:
            missing.append("subdomain" + ("(unproven)" if sd["routePresent"] is None else ""))
        if mapping == "dual" and (gw is None or gw["routePresent"] is not True):
            missing.append("gateway" + ("(unproven)" if (gw and gw["routePresent"] is None) else ""))
        ok = len(missing) == 0
        rows.append({"id": s["id"], "method": method, "host": host, "concretePath": concrete,
                     "mapping": mapping, "probeMethod": pm, "subdomain": sd, "gateway": gw,
                     "authEvidence": ev, "missingForms": missing, "pass": ok})
        print(f'{"OK " if ok else "GAP"} [{mapping}] {method} {host}{concrete}  '
              f'sd={sd["status"]}/{sd["routePresent"]}'
              + (f' gw={gw["status"]}/{gw["routePresent"]}' if gw else " gw=-")
              + (f'  MISSING={",".join(missing)}' if missing else ""), flush=True)

    gaps = [{"id": r["id"], "mapping": r["mapping"], "method": r["method"],
             "concretePath": r["concretePath"], "missingForms": r["missingForms"],
             "subdomain": {"url": r["subdomain"]["url"], "status": r["subdomain"]["status"],
                           "routePresent": r["subdomain"]["routePresent"]},
             "gateway": ({"url": r["gateway"]["url"], "status": r["gateway"]["status"],
                          "routePresent": r["gateway"]["routePresent"]} if r["gateway"] else None)}
            for r in rows if not r["pass"]]
    doc = {
        "description": "Route-existence sweep of every first-party needs-real-wallet shape against the production candidate ORIGIN "
                       f"({ORIGIN_IP}), probed via curl --resolve <vhost>:443:{ORIGIN_IP} to BYPASS Cloudflare. Reads: unauthenticated GET. "
                       "Writes: OPTIONS (never mutates). Each form judged INDEPENDENTLY; routePresent is True (service/nginx non-routing "
                       "response), False (nginx routing 404/405: text/plain 'not found'|empty or nginx html), or null=UNPROVEN "
                       "(Cloudflare/WAF signature or 502/503 upstream-down). Cloudflare responses (server: cloudflare, cf-ray, 'error code: NNNN') "
                       "are NEVER counted as present. mapping=dual requires subdomain AND gateway both True; subdomain-only requires subdomain True. "
                       "authEvidence links committed signed-flow evidence and never substitutes for a route form. Gaps are exact-path fix items "
                       "for the conformance lane and may not be waived.",
        "candidateDomain": CAND,
        "candidateOriginIp": ORIGIN_IP,
        "mappingRule": "dual => subdomainPresent AND gatewayPresent (both strictly True); subdomain-only => subdomainPresent True. "
                       "Any missing/unproven required form is a gap.",
        "sweptShapeCount": len(rows),
        "gapCount": len(gaps),
        "gaps": gaps,
        "rows": rows,
    }
    json.dump(doc, open(f"{F}/route-sweep.json","w"), indent=1)
    print(f"\n{len(rows)} shapes swept, {len(gaps)} route gaps -> route-sweep.json", flush=True)
    for g in gaps:
        print(f'  GAP [{g["mapping"]}] {g["id"]} missing={g["missingForms"]}', flush=True)

if __name__ == "__main__":
    main()
