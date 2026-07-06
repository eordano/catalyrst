use super::controls::{admin_disabled_note, controls};
use super::*;

pub async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let p = probe().await;

    let (mut configured, mut up) = (0usize, 0usize);
    for g in CATALOG {
        for s in g.services {
            if let Some(ok) = svc_health(g, s, content_healthy, &p) {
                configured += 1;
                if ok {
                    up += 1;
                }
            }
        }
    }

    let realm = state
        .realm_name
        .clone()
        .unwrap_or_else(|| "catalyrst realm".to_string());
    let (status_cls, status_txt) = if content_healthy {
        ("ok", "Online")
    } else {
        ("bad", "Degraded")
    };

    let mut b = String::new();
    b.push_str("<div class=\"hero\"><div class=\"wrap\">");
    b.push_str("<h2>A self-hosted <em>Decentraland</em> realm.</h2>");
    b.push_str("<p>catalyrst is a from-scratch Rust implementation of the Decentraland service plane — content &amp; lambdas, the explorer APIs, the social stack, the creator and marketplace planes, scene-state multiplayer and a federation layer. Everything an explorer talks to, from one workspace.</p>");
    if let Some(base) = realm_base_url(&state) {
        let play = format!(
            "https://decentraland.org/play/?realm={}",
            urlencoding::encode(&base)
        );
        b.push_str(&format!(
            "<div><a class=\"cta\" href=\"{}\" rel=\"noopener\">Open in Decentraland →</a><a class=\"cta ghost\" href=\"/about\">View realm API</a></div>",
            esc(&play)
        ));
    }
    b.push_str("<div class=\"statusbar\">");
    b.push_str(&stat(
        "realm status",
        &format!(
            "<span class=\"pill\"><span class=\"dot {status_cls}\"></span>{status_txt}</span>"
        ),
        true,
        true,
    ));
    b.push_str(&stat("realm", &esc(&realm), true, false));
    b.push_str(&stat(
        "network",
        &esc(network_name(&state.eth_network)),
        true,
        false,
    ));
    if p.activity.users.is_some() {
        b.push_str(&stat(
            "users online",
            &opt_big(p.activity.users),
            false,
            false,
        ));
    }
    b.push_str(&stat("content sync", &esc(&sync_state), true, false));
    b.push_str(&stat(
        "services healthy",
        &format!("{up}<span style=\"color:var(--mut2);font-size:18px\"> / {configured}</span>"),
        false,
        false,
    ));
    b.push_str("</div></div></div>");

    b.push_str("<main><div class=\"wrap\">");

    let a = &p.activity;
    if a.users.is_some()
        || a.peers.is_some()
        || a.hot_scenes.is_some()
        || a.ss_connections.is_some()
    {
        b.push_str("<section><div class=\"shead\"><h3>Live activity</h3><span class=\"c\">across the realm right now</span></div><div class=\"statusbar\">");
        b.push_str(&stat("users online", &opt_big(a.users), false, false));
        b.push_str(&stat("peers", &opt_big(a.peers), false, false));
        b.push_str(&stat("islands", &opt_big(a.islands), false, false));
        b.push_str(&stat("hot scenes", &opt_big(a.hot_scenes), false, false));
        b.push_str(&stat(
            "scene connections",
            &opt_big(a.ss_connections),
            false,
            false,
        ));
        b.push_str("</div></section>");
    }

    b.push_str(&format!(
        "<section><div class=\"shead\"><h3>Service plane</h3><span class=\"c\">{} bundles · {} services</span></div><div class=\"groups\">",
        CATALOG.len(),
        CATALOG.iter().map(|g| g.services.len()).sum::<usize>()
    ));
    for g in CATALOG {
        let (dc, dt) = dot(group_health(g, content_healthy, &p));
        b.push_str("<div class=\"group\"><div class=\"gh\">");
        b.push_str(&format!("<span class=\"dot {dc}\" title=\"{dt}\"></span>"));
        b.push_str(&format!(
            "<a class=\"gt\" href=\"/admin/{}\">{}</a>",
            esc(g.key),
            esc(g.title)
        ));
        b.push_str(&format!(
            "<span class=\"gp\">{}</span></div>",
            esc(g.bundle)
        ));
        for s in g.services {
            let (sc, stt) = dot(svc_health(g, s, content_healthy, &p));
            b.push_str(&format!("<div class=\"svc\"><span class=\"dot {sc}\" title=\"{stt}\"></span><div class=\"sd\">"));
            b.push_str(&format!(
                "<div class=\"sn\">{}<span class=\"ref\">{}</span></div>",
                esc(s.name),
                esc(s.reference)
            ));
            b.push_str(&format!("<div class=\"sdesc\">{}</div>", esc(s.desc)));
            if s.path.is_empty() {
                b.push_str("<span class=\"spath no\">internal</span>");
            } else {
                b.push_str(&format!(
                    "<a class=\"spath\" href=\"{0}\">{0}</a>",
                    esc(s.path)
                ));
            }
            b.push_str("</div></div>");
        }
        b.push_str("</div>");
    }
    b.push_str("</div></section>");

    b.push_str("<section><div class=\"shead\"><h3>Quick links</h3></div><div class=\"links\">");
    let links = [
        (
            "/about",
            "Realm descriptor",
            "What explorer clients fetch to join this realm",
            "/about",
        ),
        (
            "/places",
            "Places",
            "Discover places & scenes in this realm",
            "/places",
        ),
        (
            "/v1/map.png",
            "Map",
            "Genesis-city map render",
            "/v1/map.png",
        ),
        (
            "/admin",
            "Admin console",
            "Live cross-service status (loopback / private network only)",
            "/admin",
        ),
    ];
    for (href, t, d, u) in links {
        b.push_str(&format!(
            "<a class=\"link\" href=\"{}\"><div class=\"lt\">{}</div><div class=\"ld\">{}</div><div class=\"lu\">{}</div></a>",
            esc(href), esc(t), esc(d), esc(u)
        ));
    }
    b.push_str("</div></section>");
    b.push_str("</div></main>");

    page(&state, &format!("catalyrst — {realm}"), "overview", &b)
}

pub async fn admin(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Html<String> {
    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let cluster = state.content_cluster.get_status();
    let failed = state.database.get_failed_deployments().await;
    let p = probe().await;
    let a = &p.activity;
    let va = viewer_admin(&headers);

    let mut b = String::new();
    b.push_str("<main><div class=\"wrap\">");

    if va.enabled {
        b.push_str(&controls(&va, "all"));
    } else {
        b.push_str("<section><div class=\"shead\"><h3>Operator controls</h3></div>");
        b.push_str(admin_disabled_note());
        b.push_str("</section>");
    }

    b.push_str("<section><div class=\"shead\"><h3>Live activity</h3><span class=\"c\">real-time across services</span></div><div class=\"statusbar\">");
    b.push_str(&stat("users online", &opt_big(a.users), false, true));
    b.push_str(&stat("peers", &opt_big(a.peers), false, false));
    b.push_str(&stat("islands", &opt_big(a.islands), false, false));
    b.push_str(&stat("hot scenes", &opt_big(a.hot_scenes), false, false));
    b.push_str(&stat(
        "scene conns",
        &opt_big(a.ss_connections),
        false,
        false,
    ));
    b.push_str(&stat("loaded scenes", &opt_big(a.ss_scenes), false, false));
    b.push_str(&stat("AB build queue", &opt_big(a.ab_queue), false, false));
    b.push_str(&stat(
        "comms uptime",
        &a.uptime_secs.map(fmt_uptime).unwrap_or_else(|| "—".into()),
        true,
        false,
    ));
    b.push_str("</div></section>");

    b.push_str("<section><div class=\"shead\"><h3>Realm</h3></div><div class=\"grid\">");
    let kvs: [(&str, String); 8] = [
        (
            "realm name",
            state.realm_name.clone().unwrap_or_else(|| "—".into()),
        ),
        ("eth network", state.eth_network.clone()),
        ("mode", mode_str(state.is_read_only()).to_string()),
        ("content version", state.content_version.clone()),
        ("lambdas version", state.lambdas_version.clone()),
        ("commit", state.commit_hash.clone()),
        ("content url", state.content_public_url.clone()),
        ("lambdas url", state.lambdas_public_url.clone()),
    ];
    for (k, v) in kvs {
        b.push_str(&format!(
            "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
            esc(k),
            esc(if v.is_empty() { "—" } else { &v })
        ));
    }
    b.push_str("</div></section>");

    let (sc, st) = if content_healthy {
        ("ok-t", "healthy")
    } else {
        ("bad-t", "degraded")
    };
    b.push_str(&format!(
        "<section><div class=\"shead\"><h3>Synchronization</h3><span class=\"c {sc}\">{st}</span></div><div class=\"grid\">"
    ));
    b.push_str(&format!(
        "<div class=\"kv\"><div class=\"k\">state</div><div class=\"v\">{}</div></div>",
        esc(&sync_state)
    ));
    if let Value::Object(map) = &cluster {
        for (k, v) in map {
            let vs = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            b.push_str(&format!(
                "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
                esc(k),
                esc(&vs)
            ));
        }
    }
    b.push_str("</div></section>");

    b.push_str("<section><div class=\"shead\"><h3>Deployments</h3></div><div class=\"grid\">");
    let (failed_txt, failed_cls) = match &failed {
        Ok(v) if v.is_empty() => ("0".to_string(), "ok-t"),
        Ok(v) => (human(v.len() as u64), "warn-t"),
        Err(_) => ("unavailable".to_string(), "bad-t"),
    };
    b.push_str(&format!(
        "<div class=\"kv\"><div class=\"k\">failed deployments</div><div class=\"v {failed_cls}\">{failed_txt}</div></div>"
    ));
    b.push_str("</div></section>");

    b.push_str("<section><div class=\"shead\"><h3>Service health</h3><span class=\"c\">click a bundle for live detail</span></div>");
    b.push_str("<table><thead><tr><th>Bundle</th><th>Service group</th><th>Members</th><th>Health</th><th>Probe</th></tr></thead><tbody>");
    let urls = service_urls();
    for g in CATALOG {
        let h = group_health(g, content_healthy, &p);
        let (dc, dt) = dot(h);
        let members = if g.multi {
            p.groups
                .get(g.key)
                .map(|gh| {
                    let total = gh.members.len();
                    let up = gh.members.values().filter(|v| **v).count();
                    format!("{up}/{total} up")
                })
                .unwrap_or_else(|| "—".into())
        } else {
            "—".into()
        };
        let probe = if g.key == "content" {
            "local".to_string()
        } else {
            urls.get(g.key)
                .map(|u| format!("{u}/health"))
                .unwrap_or_else(|| "—".into())
        };
        b.push_str(&format!(
            "<tr><td class=\"mono\"><a href=\"/admin/{}\">{}</a></td><td>{}</td><td class=\"mono\">{}</td><td><span class=\"pill\"><span class=\"dot {dc}\"></span>{dt}</span></td><td class=\"mono\">{}</td></tr>",
            esc(g.key),
            esc(g.bundle),
            esc(g.title),
            esc(&members),
            esc(&probe)
        ));
    }
    b.push_str("</tbody></table>");
    b.push_str("<div class=\"note\">/admin is not exposed on the public edge. Reach it on the loopback port or over the private network, and front it with auth before any public exposure.</div>");
    b.push_str("</section>");

    b.push_str("</div></main>");
    page(&state, "catalyrst — admin", "admin", &b)
}

pub async fn admin_service(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let va = viewer_admin(&headers);
    let Some(g) = group_by_key(&key) else {
        let body = "<main><div class=\"wrap\"><a class=\"back\" href=\"/admin\">← back to admin</a><div class=\"empty\" style=\"padding:60px;color:var(--mut2)\">no such service</div></div></main>";
        return (
            axum::http::StatusCode::NOT_FOUND,
            page(&state, "catalyrst — not found", "admin", body),
        )
            .into_response();
    };

    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let p = probe().await;
    let (dc, dt) = dot(group_health(g, content_healthy, &p));

    let mut b = String::new();
    b.push_str("<main><div class=\"wrap\">");
    b.push_str("<a class=\"back\" href=\"/admin\">← back to admin</a>");
    b.push_str(&format!(
        "<div class=\"shead\"><h3>{}</h3><span class=\"c\">{}</span><span class=\"spacer\" style=\"flex:1\"></span><span class=\"pill\"><span class=\"dot {dc}\"></span>{dt}</span></div>",
        esc(g.title),
        esc(g.bundle)
    ));

    if va.enabled {
        b.push_str(&controls(&va, g.key));
    }

    if g.multi {
        b.push_str("<div class=\"grid\">");
        for s in g.services {
            let (mc, mt) = dot(svc_health(g, s, content_healthy, &p));
            b.push_str(&format!(
                "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\"><span class=\"pill\"><span class=\"dot {mc}\"></span>{mt}</span></div></div>",
                esc(s.name)
            ));
        }
        b.push_str("</div>");
    }

    if g.key == "content" {
        b.push_str("<div class=\"grid\">");
        let cluster = state.content_cluster.get_status();
        let kvs = [
            ("sync state", sync_state.clone()),
            ("content version", state.content_version.clone()),
            ("lambdas version", state.lambdas_version.clone()),
            ("commit", state.commit_hash.clone()),
            ("network", state.eth_network.clone()),
            ("mode", mode_str(state.is_read_only()).to_string()),
        ];
        for (k, v) in kvs {
            b.push_str(&format!(
                "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
                esc(k),
                esc(&v)
            ));
        }
        b.push_str("</div>");
        b.push_str(&format!(
            "<div class=\"raw\"><div class=\"rt\">cluster status</div><pre>{}</pre></div>",
            esc(&serde_json::to_string_pretty(&cluster).unwrap_or_default())
        ));
    } else if let Some(base) = service_urls().get(g.key) {
        b.push_str("<div class=\"eplist\">");
        for (label, path) in g.detail {
            b.push_str(&format!(
                "<a class=\"ep\" href=\"{0}{1}\" rel=\"noopener\">{2} · {1}</a>",
                esc(base),
                esc(path),
                esc(label)
            ));
        }
        b.push_str("</div>");

        let futs = g.detail.iter().map(|(label, path)| async move {
            let body = match fetch_json(&format!("{base}{path}")).await {
                Some(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
                None => match probe_up(&format!("{base}{path}")).await {
                    true => "(200 OK — non-JSON body)".into(),
                    false => "(unreachable)".into(),
                },
            };
            (*label, *path, body)
        });
        for (label, path, body) in futures::future::join_all(futs).await {
            b.push_str(&format!(
                "<div class=\"raw\"><div class=\"rt\">{} <span style=\"color:var(--mut2)\">{}</span></div><pre>{}</pre></div>",
                esc(label),
                esc(path),
                esc(&body)
            ));
        }
    } else {
        b.push_str("<div class=\"note\">This bundle has no configured URL in CATALYRST_SERVICE_URLS, so its live status can't be probed. Add a `");
        b.push_str(&esc(g.key));
        b.push_str("=http://host:port` entry to enable it.</div>");
    }

    b.push_str("</div></main>");
    page(&state, &format!("catalyrst — {}", g.title), "admin", &b).into_response()
}
