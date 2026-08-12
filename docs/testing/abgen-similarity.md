# Asset-bundle similarity gates

The similarity harness compares catalyrst-abgen's asset-bundle output
against the reference asset-bundle-converter (abc) across three tiers,
reporting a per-metric gate verdict, not one pass/fail. Ours runs as
catalyrst-abgen's warm JIT server on `:5147` (determinism runs use
`--fresh-ours` scratch servers). Thresholds live as data in
`pipeline/simval-gates.json`; the allowlist starts empty, each entry needs
a recorded cause and evidence, and a metric without evidence is NOT-RUN
rather than skipped.

## Tiers

Tier A compares against a production snapshot (`ab-cdn-reference`, 472G)
served by `pipeline/abgencompare/dirserve.py --layout ref`, plus a live
spot-check against `ab-cdn.decentraland.org` confirming the snapshot
server itself isn't what's measured. Pairing is by lowercased fileCID per
platform (`fetch.norm_bundle_name`); the snapshot covers windows and mac
only, no webgl or linux.

Tier B compares against the `abc-deterministic-guids` converter (det-guid),
the only leg able to isolate GUID-nondeterminism causality from structural
divergence; it needs a Unity editor license and a working
`convert-corpus.sh`, so it runs on a host carrying an editor.

Tier C compares rendered pixels against a `val300` compat-bag baseline,
excluded from the aggregate gate by design since rendering needs an editor
and runs on its own cadence.

## Corpus

Twelve entities, ~81 content files, verified active in the reference
snapshot: three scenes
(`bafkreieb6izdbhadi6vyjniq3hhpb363i44rf676wpjygyjrlhzsfp7eoa`;
`bafkreiegrofix7hdlrdpo44oyy5qmtcoqxle3n6imyy4urbl2d2khb3c6m`, an NC5
control; `bafkreieh2rht3iyw74sik67dsvnslbnybytg46cipiriov77jpxyxd7txq`, a
stress case); one live wearable
(`bafkreigioaz6v7zz2prui6lmipmrvuwocsmojpapu5rhw4g6mmwigkadzm`); four
purged wearables and four purged emotes — in manifests but 404 on the live
CDN, at payload versions v15 (2), v18 (4), v22 (10). All 12 count toward
manifest-set metrics; the 4 non-purged give 57 comparable payload pairs
per platform.

## Metrics

M0 byte-determinism: two scratch rebuilds, same names and sha256. M1 (L1)
manifest bundle-set equality, threshold setEqualRate ≥0.9, 0 unexplained.
M2 (L2) UnityFS parse, threshold 0 load failures. M3 (L3) raw CAB names
and path_id multiset: recorded only at Tier A (GUIDs are nondeterministic
in production by design), gated at Tier B (cabEqual 100%, pidJaccard
≥0.90). M4 (L4) objdump line multisets with pid/CAB tokens erased,
threshold fail rate ≤2%, structIdentical ≥50% at Tier A / ≥95% at Tier B,
0 unexplained signatures outside the allowlist. M5 (L5) texel comparison
via texcmp (identical / identical-decode / imperceptible ≤200ppm),
threshold ≥0.95 identical-decode. M6 (L4b) pinned metadata-line audit:
pins fall inside
{timestamp, version, date, CAB lines} for TextAsset (class 49) and
AssetBundle (class 142). M7 (L6) render comparison against val300, Tier C
only. M8 falsifiability: NC1-NC5 must all detect, or the run aborts.

## Current results

| Metric | Value | Threshold | Result |
|---|---|---|---|
| M1-fileset | setEqualRate 1.0 (12/12), 0 unpaired; bundle-filtered, raw sidecar lists differ on 8/12 | ≥0.9, 0 unexplained | PASS |
| M2-container | 0 load failures / 57 pairs | 0 | PASS |
| M3-identity | cabEqual 57/57; raw pidJaccard median 0.034 | recorded only | INFO |
| M4-structure | 39/57 structIdentical (68.4%), 18 diffs in one allowlisted `.resS bytes=#` signature, 0 fails | fail ≤2%, ident ≥50%, 0 unallowlisted | PASS |
| M5-textures | okRate 0.071 (2 identical-decode + 10 allowlisted + 16 purged of 28); plus a 4th class beyond 4-16ppm, "encoder/resize drift" (delta 88, 78) | ≥0.95 | FAIL (explained) |
| M6-metadata-pins | 114 pinned lines / 57 pairs, all class-49 timestamps inside the expected-pin set | subset rule | PASS |
| M8-falsifiability | NC1-NC4 detected; one documented vol_norm blind spot | all detected | PASS |
| Byte-identical pairs | 0/57 | not gated (GUID nondeterminism) | recorded |

The run-level gate is false, driven solely by M5; the claim that abgen
output is similar to production rests on M1/M2/M4/M6, all passing.

A live 3-entity spot-check agrees with the snapshot run on 12/12
per-bundle labels. M0a determinism passes: 54/54 bundle files match on
name and sha256 across two independent scratch runs (distinct out_roots,
ports, pids, zero shared inodes). An independent pure-Python UnityFS
re-parse reproduces cabEqual 57/57 and pid-Jaccard median 0.034483; sha256
provenance holds across the snapshot, the live CDN, and every re-fetch;
CDN HEAD probes confirm all 16 purged entities 404 and all 12 measurable
entities 200.

Tier B has not run — no host in its usual path carries a Unity editor
license. A spec-correspondence audit stands in, graded PLAUSIBLE,
never PASS: abgen and det-guid derive identical seed strings and md5-hex
for the root_hash, material, texture, metadata, and animatorController
namespaces; abgen alone also derives a mesh namespace, an expected gap
that caps the audit at PLAUSIBLE. Tier C is NOT-RUN by design.

Negative controls NC1-NC5 all fail the comparator as required: a
cross-entity mispair; a single-byte XOR (differingBytes=1,
firstDiff=66642); a 1KiB truncation (load failure rc=101); a
cross-platform same-CID pair (cabEqual=false, pidJaccard=1.0); a dropped
manifest on the stress scene (M1 FAIL, surviving allowlist triage since
setEqualRate ignores the allowlist).

## Open gaps

- The det-guid leg needs a 3-scene MANIFEST under `abc-deterministic-guids`,
  `convert-corpus.sh` (`PLATFORM=windows`, stale defaults overridden), and
  a second conversion pass for M0b — it has no evidence yet for M3 raw
  CAB/pid identity, M0b, or byteIdenticalRate.
- Emotes and 4 of 5 wearables need ~8 replacement entities,
  content-DB-active and HTTP 200 live, plus one supplemental Tier A run,
  since AnimationClip and animatorController content is never compared at
  L2-L5 today.
- L5 needs a texture-focused Tier A run against the live CDN on post-v38
  entities, since current results mostly reflect vintage production
  encoding policy (2 identical-decode of 12 measurable pairs, BC7 only); it
  also needs a texcmp maxChannelDelta guard: a single-block BC7 flip (128)
  currently scores 15.26ppm and passes as ok.
- A generality claim needs a headless breadth sweep across 200-500
  stratified entities (M1/M2/M4 only) against the live CDN: the corpus
  covers 12 of ~74,000 active entities, 79% from one stress scene, no
  multi-parcel scene represented.
- The 18 `.resS`-only pairs and 10 visible-allowlisted texture pairs need a
  render lane against the val300 baseline (`pipeline/CAMPAIGN.md`), per the
  standing load/render/behave-identical goal.
- LOD scope needs stating explicitly in any claim, or piloting as a pairing
  extension, since upstream LODs come from a separate pipeline.

Committed: the `pipeline/simval-gates.json` allowlist, the corpus files,
the harness, and per-run evidence under `crates/catalyrst-abgen/runs/`
(gitignored).
