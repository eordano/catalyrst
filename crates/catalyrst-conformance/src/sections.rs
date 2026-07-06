use anyhow::Result;
use colored::Colorize;

use crate::checks::{
    test_content_hash, test_get_bytes, test_get_json, test_pagination, test_post_json,
};
use crate::{Args, BootstrapData, Ctx, Endpoints, Scoreboard};

pub(crate) async fn run_content_section(
    ctx: &Ctx,
    ep: &Endpoints,
    bootstrap: &BootstrapData,
    score: &mut Scoreboard,
    args: &Args,
    should_run: &dyn Fn(&str) -> bool,
) -> Result<()> {
    if should_run("about") {
        println!("{}", "=== About ===".bold());
        let outcome = test_get_json(
            ctx,
            "about",
            &ep.baseline_root,
            &ep.candidate_root,
            "/about",
        )
        .await?;
        score.record_outcome(outcome, "/about (root)", args.verbose);
        ctx.sleep_between().await;
        let outcome2 = test_get_json(
            ctx,
            "about",
            &ep.baseline_content,
            &ep.candidate_content,
            "/about",
        )
        .await?;
        score.record_outcome(outcome2, "/content/about", args.verbose);
        println!();
    }

    if should_run("status") {
        println!("{}", "=== Status ===".bold());
        let outcome = test_get_json(
            ctx,
            "status",
            &ep.baseline_content,
            &ep.candidate_content,
            "/status",
        )
        .await?;
        score.record_outcome(outcome, "/content/status", args.verbose);
        println!();
    }

    if should_run("challenge") {
        println!("{}", "=== Challenge ===".bold());
        let outcome = test_get_json(
            ctx,
            "challenge",
            &ep.baseline_content,
            &ep.candidate_content,
            "/challenge",
        )
        .await?;
        score.record_outcome(outcome, "/content/challenge", args.verbose);
        println!();
    }

    if should_run("snapshots") {
        println!("{}", "=== Snapshots ===".bold());
        let outcome = test_get_json(
            ctx,
            "snapshots",
            &ep.baseline_content,
            &ep.candidate_content,
            "/snapshots",
        )
        .await?;
        score.record_outcome(outcome, "/content/snapshots", args.verbose);
        println!();
    }

    if should_run("failed-deployments") {
        println!("{}", "=== Failed Deployments ===".bold());
        let outcome = test_get_json(
            ctx,
            "failed-deployments",
            &ep.baseline_content,
            &ep.candidate_content,
            "/failed-deployments",
        )
        .await?;
        score.record_outcome(outcome, "/content/failed-deployments", args.verbose);
        println!();
    }

    if should_run("deployments") {
        println!("{}", "=== Deployments ===".bold());
        let cases = [
            (
                "profile, limit=5, DESC",
                "/deployments?entityType=profile&limit=5&sortingOrder=DESC",
            ),
            (
                "scene, limit=5, ASC",
                "/deployments?entityType=scene&limit=5&sortingOrder=ASC",
            ),
            (
                "wearable, limit=3, fields=pointers,content",
                "/deployments?entityType=wearable&limit=3&fields=pointers,content",
            ),
            (
                "emote, limit=3",
                "/deployments?entityType=emote&limit=3&sortingOrder=DESC",
            ),
        ];
        for (label, path) in &cases {
            let outcome = test_get_json(
                ctx,
                "deployments",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("Deployments ({})", label), args.verbose);
            ctx.sleep_between().await;
        }
        println!();

        println!("{}", "=== Deployments pagination (3 pages) ===".bold());
        test_pagination(
            ctx,
            &ep.baseline_content,
            &ep.candidate_content,
            score,
            args.verbose,
        )
        .await?;
        println!();
    }

    if should_run("active-entities") {
        println!("{}", "=== Active Entities ===".bold());

        for ptr in ["0,0", "-50,-50", "10,10"] {
            let body = serde_json::json!({ "pointers": [ptr] });
            let outcome = test_post_json(
                ctx,
                "active-entities",
                &ep.baseline_content,
                &ep.candidate_content,
                "/entities/active",
                &body,
            )
            .await?;
            score.record_outcome(outcome, &format!("Active by pointer {}", ptr), args.verbose);
            ctx.sleep_between().await;
        }

        if let Some(eid) = bootstrap.profile_entity_ids.first() {
            let body = serde_json::json!({ "ids": [eid] });
            let outcome = test_post_json(
                ctx,
                "active-entities",
                &ep.baseline_content,
                &ep.candidate_content,
                "/entities/active",
                &body,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Active by ID ({}...)", &eid[..eid.len().min(12)]),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("entities") {
        println!("{}", "=== /entities/{type} ===".bold());
        for ptr in &bootstrap.scene_pointers.iter().take(2).collect::<Vec<_>>() {
            let path = format!("/entities/scene?pointer={}", urlencoding::encode(ptr));
            let outcome = test_get_json(
                ctx,
                "entities",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/entities/scene?pointer={}", ptr),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(addr) = bootstrap.profile_addresses.first() {
            let path = format!("/entities/profile?pointer={}", addr);
            let outcome = test_get_json(
                ctx,
                "entities",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!(
                    "/entities/profile?pointer={}...",
                    &addr[..addr.len().min(10)]
                ),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("audit") {
        println!("{}", "=== Audit ===".bold());
        if let Some(eid) = bootstrap.profile_entity_ids.first() {
            let path = format!("/audit/profile/{}", eid);
            let outcome = test_get_json(
                ctx,
                "audit",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/audit/profile/{}...", &eid[..eid.len().min(12)]),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(eid) = bootstrap.scene_entity_ids.first() {
            let path = format!("/audit/scene/{}", eid);
            let outcome = test_get_json(
                ctx,
                "audit",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/audit/scene/{}...", &eid[..eid.len().min(12)]),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("pointer-changes") {
        println!("{}", "=== Pointer Changes ===".bold());
        for (label, path) in [
            (
                "profile, from=1700000000000, limit=10",
                "/pointer-changes?entityType=profile&from=1700000000000&limit=10",
            ),
            (
                "scene, from=1700000000000, limit=10",
                "/pointer-changes?entityType=scene&from=1700000000000&limit=10",
            ),
        ] {
            let outcome = test_get_json(
                ctx,
                "pointer-changes",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("pointer-changes ({})", label),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("content") {
        println!("{}", "=== Content bytes ===".bold());
        let hashes_to_test: Vec<&str> = bootstrap
            .content_hashes
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect();
        if hashes_to_test.is_empty() {
            score.skip("content bytes", "no content hashes bootstrapped");
        } else {
            for hash in hashes_to_test {
                let outcome =
                    test_content_hash(ctx, &ep.baseline_content, &ep.candidate_content, hash)
                        .await?;
                score.record_outcome(
                    outcome,
                    &format!("Content {}...", &hash[..hash.len().min(12)]),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        }
        println!();
    }

    if should_run("available-content") {
        println!("{}", "=== Available Content ===".bold());
        if let Some(hash) = bootstrap.content_hashes.first() {
            let path = format!("/available-content?cid={}", hash);
            let outcome = test_get_json(
                ctx,
                "available-content",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Available content ({}...)", &hash[..hash.len().min(12)]),
                args.verbose,
            );
        } else {
            score.skip("available-content", "no content hashes bootstrapped");
        }
        println!();
    }

    if should_run("active-entities-by-hash") {
        println!("{}", "=== Active Entities By Hash ===".bold());
        if let Some(hash) = bootstrap.content_hashes.first() {
            let path = format!("/contents/{}/active-entities", hash);
            let outcome = test_get_json(
                ctx,
                "active-entities-by-hash",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Active by hash ({}...)", &hash[..hash.len().min(12)]),
                args.verbose,
            );
        } else {
            score.skip("active-entities-by-hash", "no content hashes bootstrapped");
        }
        println!();
    }

    if should_run("thumbnail") {
        println!("{}", "=== Item thumbnail ===".bold());
        for pointer in [
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5:0",
        ] {
            let path = format!("/queries/items/{}/thumbnail", urlencoding::encode(pointer));
            let outcome =
                test_get_bytes(ctx, &ep.baseline_content, &ep.candidate_content, &path).await?;
            score.record_outcome(outcome, &format!("thumbnail {}", pointer), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("image") {
        println!("{}", "=== Item image ===".bold());
        for pointer in [
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5:0",
        ] {
            let path = format!("/queries/items/{}/image", urlencoding::encode(pointer));
            let outcome =
                test_get_bytes(ctx, &ep.baseline_content, &ep.candidate_content, &path).await?;
            score.record_outcome(outcome, &format!("image {}", pointer), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("erc721") {
        println!("{}", "=== ERC721 entity ===".bold());
        let paths = [
            "/queries/erc721/137/0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5/0",
            "/queries/erc721/137/0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5/0/1",
        ];
        for path in paths {
            let outcome = test_get_json(
                ctx,
                "erc721",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(outcome, path, args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("entities-by-collection") {
        println!("{}", "=== Entities by collection URN ===".bold());
        let urn = urlencoding::encode(
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5",
        );
        let path = format!("/entities/active/collections/{}", urn);
        let outcome = test_get_json(
            ctx,
            "entities-by-collection",
            &ep.baseline_content,
            &ep.candidate_content,
            &path,
        )
        .await?;
        score.record_outcome(outcome, "entities-by-collection (v2 sample)", args.verbose);
        println!();
    }

    Ok(())
}

pub(crate) async fn run_lambdas_section(
    ctx: &Ctx,
    ep: &Endpoints,
    bootstrap: &BootstrapData,
    score: &mut Scoreboard,
    args: &Args,
    should_run: &dyn Fn(&str) -> bool,
) -> Result<()> {
    if should_run("lambdas-status") {
        println!("{}", "=== /lambdas/status ===".bold());
        let outcome = test_get_json(
            ctx,
            "lambdas-status",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/status",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/status", args.verbose);
        println!();
    }

    if should_run("contracts") {
        println!("{}", "=== /lambdas/contracts/* ===".bold());
        for path in [
            "/contracts/servers",
            "/contracts/pois",
            "/contracts/denylisted-names",
        ] {
            let outcome = test_get_json(
                ctx,
                "contracts",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("third-party-integrations") {
        println!("{}", "=== /lambdas/third-party-integrations ===".bold());
        let outcome = test_get_json(
            ctx,
            "third-party-integrations",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/third-party-integrations",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/third-party-integrations", args.verbose);
        println!();
    }

    if should_run("collections") {
        println!("{}", "=== /lambdas/collections (catalog) ===".bold());
        for path in [
            "/collections/wearables?pageSize=5",
            "/collections/emotes?pageSize=5",
        ] {
            let outcome = test_get_json(
                ctx,
                "collections",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("nfts-collections") {
        println!("{}", "=== /lambdas/nfts/collections ===".bold());
        let outcome = test_get_json(
            ctx,
            "nfts-collections",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/nfts/collections",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/nfts/collections", args.verbose);
        println!();
    }

    let address_opt = bootstrap.profile_addresses.first().cloned();

    if should_run("profiles") {
        println!("{}", "=== /lambdas/profiles ===".bold());
        if let Some(addr) = &address_opt {
            let outcome_get = test_get_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/profiles/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome_get,
                &format!("GET /lambdas/profiles/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
            ctx.sleep_between().await;

            let outcome_alias = test_get_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/profile/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome_alias,
                &format!("GET /lambdas/profile/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
            ctx.sleep_between().await;

            let body = serde_json::json!({ "ids": [addr] });
            let outcome_post = test_post_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                "/profiles",
                &body,
            )
            .await?;
            score.record_outcome(
                outcome_post,
                "POST /lambdas/profiles (single id)",
                args.verbose,
            );
        } else {
            score.skip("profiles", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("user-items") {
        println!("{}", "=== /lambdas/users/{address}/* ===".bold());
        if let Some(addr) = &address_opt {
            for sub in [
                "/wearables?pageSize=5",
                "/emotes?pageSize=5",
                "/third-party-wearables?pageSize=5",
                "/names?pageSize=5",
                "/lands?pageSize=5",
                "/lands-permissions",
            ] {
                let path = format!("/users/{}{}", addr, sub);
                let outcome = test_get_json(
                    ctx,
                    "user-items",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(
                    outcome,
                    &format!("/lambdas/users/{}...{}", &addr[..addr.len().min(10)], sub),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        } else {
            score.skip("user-items", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("collections-by-owner") {
        println!("{}", "=== /lambdas/collections/*-by-owner ===".bold());
        if let Some(addr) = &address_opt {
            for sub in ["wearables-by-owner", "emotes-by-owner"] {
                let path = format!("/collections/{}/{}", sub, addr);
                let outcome = test_get_json(
                    ctx,
                    "collections-by-owner",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
                ctx.sleep_between().await;
            }
        } else {
            score.skip("collections-by-owner", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("explorer") {
        println!("{}", "=== /lambdas/explorer/{address}/* ===".bold());
        if let Some(addr) = &address_opt {
            for sub in ["/wearables?pageSize=5", "/emotes?pageSize=5"] {
                let path = format!("/explorer/{}{}", addr, sub);
                let outcome = test_get_json(
                    ctx,
                    "explorer",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(
                    outcome,
                    &format!(
                        "/lambdas/explorer/{}...{}",
                        &addr[..addr.len().min(10)],
                        sub
                    ),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        } else {
            score.skip("explorer", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("parcel") {
        println!(
            "{}",
            "=== /lambdas/parcels and parcel-permissions ===".bold()
        );
        let parcels = [(0i32, 0i32), (10, 10), (-50, -50)];
        for (x, y) in parcels {
            let outcome = test_get_json(
                ctx,
                "parcel",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/parcels/{}/{}/operators", x, y),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/parcels/{}/{}/operators", x, y),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(addr) = &address_opt {
            let outcome = test_get_json(
                ctx,
                "parcel",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/users/{}/parcels/0/0/permissions", addr),
            )
            .await?;
            score.record_outcome(
                outcome,
                "/lambdas/users/<addr>/parcels/0/0/permissions",
                args.verbose,
            );
        }
        println!();
    }

    if should_run("name-owner") {
        println!("{}", "=== /lambdas/names/{name}/owner ===".bold());
        for name in ["spectabilis", "definitely-not-a-real-name-xxz"] {
            let outcome = test_get_json(
                ctx,
                "name-owner",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/names/{}/owner", name),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/names/{}/owner", name),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("outfits") {
        println!("{}", "=== /lambdas/outfits/{id} ===".bold());
        if let Some(addr) = &address_opt {
            let outcome = test_get_json(
                ctx,
                "outfits",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/outfits/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/outfits/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
        } else {
            score.skip("outfits", "no profile addresses bootstrapped");
        }
        println!();
    }

    Ok(())
}
