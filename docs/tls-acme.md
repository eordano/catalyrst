# TLS, ACME, Cloudflare DNS-01

The apex + wildcard cert is issued via Let's Encrypt **DNS-01** through
Cloudflare, never HTTP-01.

## Why DNS-01

- Issues successfully even when the zone is proxied (orange-cloud) and the
  A record points at Cloudflare — HTTP-01 would never reach the origin.
- Required for the wildcard SAN (`*.example.com`); HTTP-01 cannot
  issue wildcards.

## Credentials

The Cloudflare token (scope: Zone:DNS:Edit) lives at
`/var/lib/secrets/cloudflare-dns.env` as `CF_DNS_API_TOKEN=...`, mode 0600,
outside the nix store. ACME's `environmentFile = ...` reads it at renewal.

## Reload

`postRun = "systemctl reload nginx.service || true"` — `|| true` because a
transient reload failure shouldn't fail the renewal; the next request will
retry.

## Operator-specific

The ACME registration email is operator-specific. Set it through the module's
option surface — `services.catalyrst.acmeEmail = "ops@yourdomain.example";`
(see `nixos/configuration.nix:91` for the option, and `nixos/module-example.nix`
for a wired-up example). The module then plumbs it into
`security.acme.defaults.email`; you may set that key directly instead if you
prefer, but using `acmeEmail` keeps the option surface coherent.
