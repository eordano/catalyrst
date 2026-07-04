# catalyrst-communities — HTTP route inventory

Ported from `decentraland/social-service-ea/src/controllers/routes/http.routes.ts`,
filtered to **community routes only**. Friends, referrals, user-mutes, and
SQS handlers are out of scope for this crate.

Listening port: **`5136`** (the deployment's assigned port).

`Sign` column:
  - `optional`  — `signedFetchMiddleware({ optional: true })`: handler runs
    with or without an auth chain; behaviour differs by `verification?.auth`.
  - `required`  — `signedFetchMiddleware()`: handler returns 401 without a
    valid auth chain.
  - `bearer`    — `bearerTokenMiddleware(API_ADMIN_TOKEN)`: handler requires
    `Authorization: Bearer …`.

`Status` column:
  - `GET`   — implemented (read-path), queries the local `communities` DB.
  - `WRITE` — federation write path implemented; accepts `Signed<T>`,
    verifies + replay-checks + authority-gates + persists. Returns
    `200 { ok: true, signature_hash }` on success.
  - `501`   — surface is not modelled as a `Signed<T>` in v1 (see
    TODO.md "Deferred to v2").

## Reads (GET) — 17

| Method | Path                                                     | Sign     | Status |
|--------|----------------------------------------------------------|----------|--------|
| GET    | `/v1/communities`                                        | optional | GET    |
| GET    | `/v1/communities/{id}`                                   | optional | GET    |
| GET    | `/v1/communities/{id}/members`                           | optional | GET    |
| GET    | `/v1/communities/{id}/bans`                              | required | GET    |
| GET    | `/v1/communities/{id}/places`                            | optional | GET    |
| GET    | `/v1/communities/{id}/posts`                             | optional | GET    |
| GET    | `/v1/communities/{id}/requests`                          | required | GET    |
| GET    | `/v1/communities/{address}/managed`                      | bearer   | GET    |
| GET    | `/v1/members/{address}/communities`                      | required | GET    |
| GET    | `/v1/members/{address}/requests`                         | required | GET    |
| GET    | `/v1/members/{address}/invites`                          | required | GET    |
| GET    | `/v1/community-voice-chats/active`                       | required | GET    |
| GET    | `/v1/moderation/communities`                             | required | GET    |
| GET    | `/ping`                                                  | none     | GET    |

## Writes — 17 federation-signed, 1 bearer-gated

| Method | Path                                                     | Sign     | Schema (Signed<T>)             | Status |
|--------|----------------------------------------------------------|----------|--------------------------------|--------|
| POST   | `/v1/communities`                                        | required | `CommunityCreate`              | WRITE  |
| PUT    | `/v1/communities/{id}`                                   | required | `CommunityUpdate`              | WRITE  |
| PATCH  | `/v1/communities/{id}`                                   | required | `CommunityUpdate`              | WRITE  |
| DELETE | `/v1/communities/{id}`                                   | required | `CommunityDelete`              | WRITE  |
| POST   | `/v1/communities/{id}/members`                           | required | `CommunityJoin`                | WRITE  |
| DELETE | `/v1/communities/{id}/members/{memberAddress}`           | required | `CommunityLeave`               | WRITE  |
| PATCH  | `/v1/communities/{id}/members/{address}`                 | required | `CommunityRole`                | WRITE  |
| POST   | `/v1/communities/{id}/members/{memberAddress}/bans`      | required | `CommunityBan`                 | WRITE  |
| DELETE | `/v1/communities/{id}/members/{memberAddress}/bans`      | required | `CommunityUnban`               | WRITE  |
| POST   | `/v1/communities/{id}/places`                            | required | `CommunityPlacesAdd`           | WRITE  |
| DELETE | `/v1/communities/{id}/places/{placeId}`                  | required | `CommunityPlaceRemove`         | WRITE  |
| POST   | `/v1/communities/{id}/posts`                             | required | `CommunityPost`                | WRITE  |
| DELETE | `/v1/communities/{id}/posts/{postId}`                    | required | `CommunityPostDelete`          | WRITE  |
| POST   | `/v1/communities/{id}/posts/{postId}/like`               | required | `CommunityPostLike`            | WRITE  |
| DELETE | `/v1/communities/{id}/posts/{postId}/like`               | required | `CommunityPostUnlike`          | WRITE  |
| POST   | `/v1/communities/{id}/requests`                          | required | `CommunityRequest`             | 501    |
| PATCH  | `/v1/communities/{id}/requests/{requestId}`              | required | `CommunityRequestStatusUpdate` | WRITE  |
| POST   | `/v1/members/{address}/communities`                      | bearer   | n/a (admin batch read alias)   | 501    |

## Federation endpoints — 2

| Method | Path                                              | Sign | Status |
|--------|---------------------------------------------------|------|--------|
| GET    | `/federation/communities/snapshot`                | none | GET    |
| GET    | `/federation/communities/changes?since=&limit=`   | none | GET    |

## Excluded from this crate (intentionally)

These are in upstream `http.routes.ts` but are not community routes:

- `POST/PATCH/GET /v1/referral-progress`
- `POST /v1/referral-email`
- `GET/POST/DELETE /v1/mutes` (global user mutes, unrelated to community state)

## Totals

- **Total routes inventoried:** 35
- **GETs ported:** 14 (+ 1 ping handler = 15 route entries; ping has no auth)
- **Writes stubbed (501):** 18 (`POST` + `PUT` + `PATCH` + `DELETE` + the
  bearer-token batch read endpoint that needs admin-token wiring out of scope).
