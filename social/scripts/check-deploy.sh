#!/usr/bin/env bash
# Runs isolated proxy/backend processes. Never touches the live nginx or systemd.
set -euo pipefail
social_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
nginx_bin="${NGINX_BIN:-nginx}"
api_bin="${SOCIAL_TEST_BINARY:-$social_dir/../target/release/dcl-social-api}"
check_dir="$(mktemp -d /tmp/dcl-social-nginx.XXXXXX)"
api_pid=""; nginx_pid=""
cleanup() {
  [[ -z "$nginx_pid" ]] || kill "$nginx_pid" 2>/dev/null || true
  [[ -z "$api_pid" ]] || kill "$api_pid" 2>/dev/null || true
  [[ -z "$nginx_pid" ]] || wait "$nginx_pid" 2>/dev/null || true
  [[ -z "$api_pid" ]] || wait "$api_pid" 2>/dev/null || true
  rm -rf -- "$check_dir"
}
trap cleanup EXIT
read -r chat_api_port chat_path_port chat_host_port < <(python3 - <<'PY'
import socket
sockets=[socket.socket() for _ in range(3)]
for sock in sockets: sock.bind(('127.0.0.1',0))
print(*(sock.getsockname()[1] for sock in sockets))
PY
)
sed "s/127.0.0.1:5191/127.0.0.1:$chat_api_port/g" "$social_dir/deploy/nginx-path.conf" > "$check_dir/path.conf"
sed -e "s/127.0.0.1:5191/127.0.0.1:$chat_api_port/g" -e "s/listen 5080;/listen 127.0.0.1:$chat_host_port;/" "$social_dir/deploy/nginx-host.conf" > "$check_dir/host.conf"
cat > "$check_dir/nginx.conf" <<CONF
pid $check_dir/nginx.pid;
error_log stderr;
events {}
http {
    access_log off;
    client_body_temp_path $check_dir/client;
    proxy_temp_path $check_dir/proxy;
    fastcgi_temp_path $check_dir/fastcgi;
    uwsgi_temp_path $check_dir/uwsgi;
    scgi_temp_path $check_dir/scgi;
    include $social_dir/deploy/nginx-http.conf;
    server {
        listen 127.0.0.1:$chat_path_port;
        server_name example.com;
        location = / { return 200 'existing site'; }
        include $check_dir/path.conf;
    }
    include $check_dir/host.conf;
}
CONF
"$nginx_bin" -e stderr -p "$check_dir/" -c "$check_dir/nginx.conf" -t
sed -e "s|/opt/dcl-social/current/bin/dcl-social-api|$api_bin|" -e "s|WorkingDirectory=.*|WorkingDirectory=$social_dir|" "$social_dir/deploy/dcl-social.service" > "$check_dir/dcl-social.service"
systemd-analyze verify "$check_dir/dcl-social.service"
SOCIAL_BIND="127.0.0.1:$chat_api_port" SOCIAL_DATABASE="$check_dir/chat.sqlite" SOCIAL_ASSETS="$social_dir/dist" "$api_bin" > "$check_dir/backend.log" 2>&1 &
api_pid=$!
"$nginx_bin" -e stderr -p "$check_dir/" -c "$check_dir/nginx.conf" -g 'daemon off;' > "$check_dir/nginx.log" 2>&1 &
nginx_pid=$!
for attempt in {1..50}; do
  if curl -fsS "http://127.0.0.1:$chat_path_port/chat/api/health" > "$check_dir/health.json" 2>/dev/null; then break; fi
  sleep 0.1
done
[[ "$(curl -fsS "http://127.0.0.1:$chat_path_port/")" == 'existing site' ]]
[[ "$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$chat_path_port/chat")" == 308 ]]
curl -fsS "http://127.0.0.1:$chat_path_port/chat/" > "$check_dir/index.html"
python3 - "$check_dir/index.html" <<'PY'
import sys
html=open(sys.argv[1]).read()
assert 'src="./assets/' in html
assert 'href="./assets/' in html
PY
for host in chat.example.com dcl.social; do
  curl -fsS -H "Host: $host" "http://127.0.0.1:$chat_host_port/api/health" > /dev/null
done
echo 'PASS: systemd unit, nginx syntax, existing root preserved, /chat redirect, prefixed API/assets, and hostname routes.'
