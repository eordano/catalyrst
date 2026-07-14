---
id: landings-invite-referral
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A personalized invite landing that resolves the referrer's Decentraland
    profile and greets the visitor by the referrer's name ("MetaPioneer invited
    you to join Decentraland") converts more invite-link visitors into download
    clicks than a generic, un-personalized "join Decentraland" hero.
  because: >-
    Social proof from a named, real friend lowers the trust barrier for a
    first-time visitor: seeing who invited them -- resolved live from the
    referrer's Profile lambdas -- makes the download CTA feel like accepting a
    personal invitation rather than a cold marketing prompt, so a larger share
    of invite-link views turn into a download click.
metric:
  primary: lp_invite_download_ctr
  guardrails:
    - lp_invite_viewed
experiment:
  key: lp_invite_referral
  unit: session
  variants:
    - id: personalized_referrer
      weight: 1
      flags:
        showReferrerName: true
        resolveProfile: true
  baseline: 0.18
  mde: 0.04
decision:
  rule: >-
    Ship the personalized invite landing if lp_invite_download_ctr
    (lp_invite_download_clicked / lp_invite_viewed) clears the 0.18 baseline with
    95% confidence and neither guardrail (view volume, FAQ engagement) regresses;
    otherwise iterate on hero copy / referrer-resolution coverage.
---

# Story -- Invite / referral landing (`/landings/invite-referral?referrer=...`)

A server-rendered invite landing that personalizes the join experience around a
**referrer**. It mirrors the real `decentraland.org/invite/:referrer` page
(`StInvite`) and links the referral rewards mechanic shown on the owner profile
(`StProfileReferralRewardsTab`).

The filesystem route is `app/routes/landings.invite-referral.tsx`
(-> `/landings/invite-referral`); the referrer is carried as **`?referrer=`**
(a wallet address or a NAME handle) so it is URL-addressable for screenshots.

## Data source

- **Referrer profile** -- resolved LIVE from the Catalyst Profile lambdas
  (`GET /lambdas/profiles/{address}`, public) when `?referrer=` is an address;
  the avatar's claimed NAME (or a truncated address) becomes the hero greeting.
  A NAME handle in the param is trusted as the label; with no param the fixture's
  example referrer (`MetaPioneer`) is shown.
- **Referral state + tiers** -- from the bundled fixture
  `app/fixtures/landings-invite-referral.json`. The catalyrst-credits crate
  (`catalyrst-credits/src/lib.rs`: `/credits/seasons`,
  `/credits/users/{wallet}/progress`) is auth-chain gated (`signer_from`) and has
  no public referral-progress route yet, so the SSR site falls back to the
  fixture. Shapes mirror `decentraland/schemas` REFERRAL metadata
  (`tier`/`invitedUsers`/`rarity`) and the ui3 9-tier reward ladder.

## Journey (URL-addressable steps)

1. **view** -- `GET /landings/invite-referral`. Renders the `StInvite` hero +
   secondary hero + FAQ accordion. Emits `lp_invite_viewed`
   (`{ has_referrer, resolved, referrer_handle }`).
2. **referrer-resolved** -- `?referrer=<address|name>`. The loader resolves the
   referrer (live Profile lambdas for an address; the raw handle otherwise) and
   greets the visitor by name. Emits `lp_invite_referrer_resolved`
   (`{ referrer_handle, resolved, source }`). The **referral attribution write**
   (binding this session to the referrer) is **SIMULATED**: the loader computes
   the attribution payload but performs no on-chain / backend write.
3. **download-cta** -- clicking the "JOIN NOW" / "JOIN FREE" download CTA emits
   `lp_invite_download_clicked` (`{ cta, referrer_handle }`) and follows the real
   `decentraland.org/download` link. Deep-link `?step=download` scrolls/focuses
   the primary CTA for screenshots.
3b. **referral-rewards** -- `?step=rewards` swaps the body for the
   `StProfileReferralRewardsTab` view (the referrer's reward journey), so the
   tier ladder the invite feeds is screenshotable. Emits
   `lp_invite_rewards_viewed` (`{ accepted, current_tier }`).
4. **faqs** -- opening an FAQ accordion row emits `lp_invite_faq_opened`
   (`{ index, question }`). `?step=faqs` deep-links to the FAQ section.

## Metric

Primary `lp_invite_download_ctr = lp_invite_download_clicked / lp_invite_viewed`.

## Notes

Single-variant story (`personalized_referrer`): this is a **"spec"** surface
(loader + components from `loaderData`, not an XState A/B wizard), so the
experiment block documents the shipped, personalized behavior and exposure
attribution rather than splitting traffic. The download CTA is a real off-site
link to `decentraland.org/download`. The only **SIMULATED** piece is the referral
attribution write in step 2 (flow / states / metrics are all real); the referral
tier state is served from the fixture because the live referral-progress route is
not yet public on catalyrst-credits.
