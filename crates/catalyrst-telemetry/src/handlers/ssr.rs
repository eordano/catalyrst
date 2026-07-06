use axum::extract::{OriginalUri, Path, Query, State};
use axum::response::Html;
use serde_json::{json, Map, Value};

use crate::handlers::dashboard;
use crate::AppState;

const TEMPLATE: &str = include_str!("../dashboard.html");

#[derive(Clone, Copy, PartialEq)]
enum Surface {
    Errors,
    Metrics,
    Health,
    Flags,
    Sql,
    Session,
}

struct Route {
    surface: Surface,
    tab: &'static str,
    fingerprint: Option<String>,
    session_id: Option<String>,
}

fn parse_path(path: &str) -> Route {
    let path = {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            "/"
        } else {
            trimmed
        }
    };
    let mut r = Route {
        surface: Surface::Errors,
        tab: "issues",
        fingerprint: None,
        session_id: None,
    };
    if let Some(id) = path.strip_prefix("/session/") {
        r.surface = Surface::Session;
        r.session_id = Some(urldecode(id));
    } else if let Some(fp) = path.strip_prefix("/issues/") {
        r.surface = Surface::Errors;
        r.tab = "issues";
        r.fingerprint = Some(urldecode(fp));
    } else {
        match path {
            "/" => {}
            "/events" => r.tab = "events",
            "/metrics" => r.surface = Surface::Metrics,
            "/metrics/stream" => {
                r.surface = Surface::Metrics;
                r.tab = "events";
            }
            "/metrics/funnel" => {
                r.surface = Surface::Metrics;
                r.tab = "funnel";
            }
            "/metrics/breakdown" => {
                r.surface = Surface::Metrics;
                r.tab = "breakdown";
            }
            "/health" => r.surface = Surface::Health,
            "/flags" => r.surface = Surface::Flags,
            "/sql" => r.surface = Surface::Sql,
            _ => {}
        }
    }
    r
}

struct Filters {
    hours: i64,
    q: Option<String>,
    level: Option<String>,
    kind: Option<String>,
    env: Option<String>,
    release: Option<String>,
    tag: Option<String>,
    sort: Option<String>,
    status: Option<String>,
}

fn query_get(q: &[(String, String)], key: &str) -> Option<String> {
    q.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn parse_filters(q: &[(String, String)], surface: Surface) -> Filters {
    let metrics = surface == Surface::Metrics;
    Filters {
        hours: query_get(q, "hours")
            .and_then(|s| s.parse().ok())
            .unwrap_or(24),
        q: query_get(q, "q"),
        level: query_get(q, "level"),

        kind: query_get(q, "kind").or_else(|| {
            if surface == Surface::Errors {
                Some("event".to_string())
            } else {
                None
            }
        }),
        env: query_get(q, "env"),
        release: query_get(q, "release"),
        tag: query_get(q, "tag"),
        sort: query_get(q, "sort")
            .or_else(|| Some(if metrics { "frequent" } else { "recent" }.to_string())),
        status: query_get(q, "status")
            .or_else(|| Some(if metrics { "all" } else { "unresolved" }.to_string())),
    }
}

fn qs(
    f: &Filters,
    surface: Surface,
    fingerprint: &Option<String>,
    extra: &[(&str, String)],
) -> String {
    let metrics = surface == Surface::Metrics;
    let mut p: Vec<(String, String)> = Vec::new();
    p.push(("hours".into(), f.hours.to_string()));
    p.push((
        "source".into(),
        if metrics { "segment" } else { "sentry" }.into(),
    ));
    if let Some(v) = nonempty(&f.q) {
        p.push(("q".into(), v));
    }
    if let Some(v) = nonempty(&f.level) {
        p.push(("level".into(), v));
    }
    if let Some(v) = nonempty(&f.kind) {
        p.push(("kind".into(), v));
    }
    if let Some(v) = nonempty(&f.env) {
        p.push(("environment".into(), v));
    }
    if let Some(v) = nonempty(&f.release) {
        p.push(("release".into(), v));
    }
    if let Some(v) = nonempty(&f.tag) {
        p.push(("tag".into(), v));
    }
    if let Some(v) = nonempty(&f.sort) {
        if v != "recent" {
            p.push(("sort".into(), v));
        }
    }
    if !metrics {
        if let Some(v) = nonempty(&f.status) {
            if v != "all" {
                p.push(("status".into(), v));
            }
        }
    }
    if let Some(fp) = nonempty(fingerprint) {
        p.push(("fingerprint".into(), fp));
    }
    for (k, v) in extra {
        p.push((k.to_string(), v.clone()));
    }
    p.iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn nonempty(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|v| !v.is_empty()).cloned()
}

fn query_of<T: serde::de::DeserializeOwned>(v: Value) -> Query<T> {
    Query(serde_json::from_value(v).expect("ssr: query synth"))
}

pub async fn page(State(st): State<AppState>, OriginalUri(uri): OriginalUri) -> Html<String> {
    let path = uri.path().to_string();
    let raw_query: Vec<(String, String)> = uri.query().map(parse_query).unwrap_or_default();
    let route = parse_path(&path);
    let filters = parse_filters(&raw_query, route.surface);

    let mut cache: Map<String, Value> = Map::new();
    let mut total_html = String::new();
    let mut thead_html = String::new();
    let mut rows_html = String::new();
    let mut panel_html = String::new();
    let mut ititle_html = String::new();
    let mut imeta_html = String::new();
    let mut ihead_show = false;

    let panel_surface = matches!(
        route.surface,
        Surface::Health | Surface::Flags | Surface::Sql | Surface::Session
    ) || matches!(route.tab, "funnel" | "breakdown");

    match route.surface {
        Surface::Errors => {
            let stats = call_stats(&st, &filters, &route.fingerprint, "sentry").await;
            cache.insert(
                format!(
                    "/dash/stats?{}",
                    qs(&filters, route.surface, &route.fingerprint, &[])
                ),
                stats.clone(),
            );
            total_html = errors_total(&stats, filters.hours);

            if let Some(fp) = &route.fingerprint {
                let issue = call_events(
                    &st,
                    &filters,
                    route.surface,
                    &route.fingerprint,
                    &[("group", "1".into()), ("limit", "1".into())],
                )
                .await;
                cache.insert(
                    format!(
                        "/dash/events?{}",
                        qs(
                            &filters,
                            route.surface,
                            &route.fingerprint,
                            &[("group", "1".into()), ("limit", "1".into())]
                        )
                    ),
                    issue.clone(),
                );
                let meta = issue
                    .get("items")
                    .and_then(|i| i.as_array())
                    .and_then(|a| a.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                let (t, m) = issue_head(&meta, fp);
                ititle_html = t;
                imeta_html = m;
                ihead_show = true;

                let events = call_events(
                    &st,
                    &filters,
                    route.surface,
                    &route.fingerprint,
                    &[
                        ("group", "0".into()),
                        ("limit", "100".into()),
                        ("offset", "0".into()),
                    ],
                )
                .await;
                cache.insert(
                    format!(
                        "/dash/events?{}",
                        qs(
                            &filters,
                            route.surface,
                            &route.fingerprint,
                            &[
                                ("group", "0".into()),
                                ("limit", "100".into()),
                                ("offset", "0".into())
                            ]
                        )
                    ),
                    events.clone(),
                );
                thead_html = errors_thead(false, &filters);
                rows_html = render_rows(&events, false, false);
            } else {
                let issues = route.tab == "issues";
                let group = if issues { "1" } else { "0" };
                let events = call_events(
                    &st,
                    &filters,
                    route.surface,
                    &None,
                    &[
                        ("group", group.into()),
                        ("limit", "100".into()),
                        ("offset", "0".into()),
                    ],
                )
                .await;
                cache.insert(
                    format!(
                        "/dash/events?{}",
                        qs(
                            &filters,
                            route.surface,
                            &None,
                            &[
                                ("group", group.into()),
                                ("limit", "100".into()),
                                ("offset", "0".into())
                            ]
                        )
                    ),
                    events.clone(),
                );
                thead_html = errors_thead(issues, &filters);
                rows_html = render_rows(&events, issues, false);
            }
        }
        Surface::Metrics if matches!(route.tab, "funnel" | "breakdown") => {
            let metrics = call_metrics(&st, filters.hours).await;
            cache.insert(
                format!("/dash/metrics?hours={}", filters.hours),
                metrics.clone(),
            );
            total_html = metrics_total(&metrics, filters.hours);
        }
        Surface::Metrics => {
            let metrics = call_metrics(&st, filters.hours).await;
            cache.insert(
                format!("/dash/metrics?hours={}", filters.hours),
                metrics.clone(),
            );
            total_html = metrics_total(&metrics, filters.hours);
            let issues = route.tab == "issues";
            let group = if issues { "1" } else { "0" };
            let events = call_events(
                &st,
                &filters,
                route.surface,
                &None,
                &[
                    ("group", group.into()),
                    ("limit", "100".into()),
                    ("offset", "0".into()),
                ],
            )
            .await;
            cache.insert(
                format!(
                    "/dash/events?{}",
                    qs(
                        &filters,
                        route.surface,
                        &None,
                        &[
                            ("group", group.into()),
                            ("limit", "100".into()),
                            ("offset", "0".into())
                        ]
                    )
                ),
                events.clone(),
            );
            thead_html = metrics_thead(issues, &filters);
            rows_html = render_rows(&events, issues, true);
        }
        Surface::Health => {
            let v = call_health(&st, filters.hours).await;
            cache.insert(format!("/dash/health?hours={}", filters.hours), v.clone());
            panel_html = render_health(&v);
        }
        Surface::Flags => {
            let v = call_flags(&st).await;
            cache.insert("/dash/flags".to_string(), v.clone());
            panel_html = render_flags(&v);
        }
        Surface::Sql => {}
        Surface::Session => {
            if let Some(id) = &route.session_id {
                if let Ok(n) = id.parse::<i64>() {
                    let v = call_session(&st, n).await;
                    cache.insert(format!("/dash/session/{id}"), v.clone());
                    panel_html = render_session(&v, id);
                }
            }
        }
    }

    let boot = json!({ "cache": Value::Object(cache) });
    let boot_script = format!("<script>window.__BOOT__={};</script>", boot);

    let mut html = TEMPLATE.to_string();
    html = html.replace("<!--SSR:boot-->", &boot_script);
    html = html.replace("<!--SSR:total-->", &total_html);
    html = html.replace("<!--SSR:thead-->", &thead_html);
    html = html.replace("<!--SSR:rows-->", &rows_html);
    html = html.replace("<!--SSR:panel-->", &panel_html);
    html = html.replace("<!--SSR:ititle-->", &ititle_html);
    html = html.replace("<!--SSR:imeta-->", &imeta_html);

    let no_chrome = matches!(
        route.surface,
        Surface::Health | Surface::Flags | Surface::Sql | Surface::Session
    );
    html = html.replace(
        "<!--SSR:tableview-style-->",
        if panel_surface { "display:none" } else { "" },
    );
    html = html.replace(
        "<!--SSR:panel-style-->",
        if panel_surface {
            "display:block"
        } else {
            "display:none"
        },
    );
    html = html.replace(
        "<!--SSR:stats-style-->",
        if no_chrome { "display:none" } else { "" },
    );
    html = html.replace(
        "<!--SSR:toolbar-style-->",
        if no_chrome { "display:none" } else { "" },
    );
    html = html.replace(
        "<!--SSR:ihead-show-->",
        if ihead_show { "show" } else { "" },
    );

    Html(html)
}

async fn call_stats(st: &AppState, f: &Filters, fp: &Option<String>, source: &str) -> Value {
    let mut body = json!({ "hours": f.hours, "source": source });
    if let Some(fp) = nonempty(fp) {
        body["fingerprint"] = json!(fp);
    }
    dashboard::stats(State(st.clone()), query_of::<dashboard::StatsParams>(body))
        .await
        .map(|j| j.0)
        .unwrap_or(json!({}))
}

async fn call_metrics(st: &AppState, hours: i64) -> Value {
    dashboard::metrics(
        State(st.clone()),
        query_of::<dashboard::StatsParams>(json!({ "hours": hours })),
    )
    .await
    .map(|j| j.0)
    .unwrap_or(json!({}))
}

async fn call_events(
    st: &AppState,
    f: &Filters,
    surface: Surface,
    fp: &Option<String>,
    extra: &[(&str, String)],
) -> Value {
    let source = if surface == Surface::Metrics {
        "segment"
    } else {
        "sentry"
    };
    let mut body = json!({ "hours": f.hours, "source": source });

    if let Some(v) = nonempty(&f.q) {
        body["q"] = json!(v);
    }
    if let Some(v) = nonempty(&f.level) {
        body["level"] = json!(v);
    }
    if let Some(v) = nonempty(&f.kind) {
        body["kind"] = json!(v);
    }
    if let Some(v) = nonempty(&f.env) {
        body["environment"] = json!(v);
    }
    if let Some(v) = nonempty(&f.release) {
        body["release"] = json!(v);
    }
    if let Some(v) = nonempty(&f.tag) {
        body["tag"] = json!(v);
    }
    if let Some(v) = nonempty(&f.sort) {
        if v != "recent" {
            body["sort"] = json!(v);
        }
    }
    if let Some(v) = nonempty(&f.status) {
        if v != "all" {
            body["status"] = json!(v);
        }
    }
    if let Some(v) = nonempty(fp) {
        body["fingerprint"] = json!(v);
    }
    for (k, v) in extra {
        let obj = body.as_object_mut().unwrap();

        if let Ok(n) = v.parse::<i64>() {
            obj.insert(k.to_string(), json!(n));
        } else {
            obj.insert(k.to_string(), json!(v));
        }
    }
    dashboard::events(State(st.clone()), query_of::<dashboard::ListParams>(body))
        .await
        .map(|j| j.0)
        .unwrap_or(json!({ "items": [] }))
}

async fn call_health(st: &AppState, hours: i64) -> Value {
    dashboard::health(
        State(st.clone()),
        query_of::<dashboard::HealthParams>(json!({ "hours": hours })),
    )
    .await
    .map(|j| j.0)
    .unwrap_or(json!({}))
}

async fn call_flags(st: &AppState) -> Value {
    dashboard::flags(State(st.clone()))
        .await
        .map(|j| j.0)
        .unwrap_or(json!({}))
}

async fn call_session(st: &AppState, id: i64) -> Value {
    dashboard::session(State(st.clone()), Path(id))
        .await
        .map(|j| j.0)
        .unwrap_or(json!({ "user": Value::Null, "events": [] }))
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn lvl_class(l: &str) -> &'static str {
    match l.to_lowercase().as_str() {
        "error" => "error",
        "fatal" => "fatal",
        "warning" => "warning",
        "info" => "info",
        "debug" => "debug",
        _ => "none",
    }
}

fn short_time(ts: &str) -> String {
    if ts.len() >= 19 && ts.as_bytes().get(10) == Some(&b'T') {
        ts[11..19].to_string()
    } else {
        ts.to_string()
    }
}

fn vstr(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}
fn vi64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

fn commas(n: i64) -> String {
    let neg = n < 0;
    let digits = n.abs().to_string();
    let mut out = String::new();
    let b = digits.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

fn errors_total(stats: &Value, hours: i64) -> String {
    let total = vi64(stats, "total");
    let h = stats.get("hours").and_then(|x| x.as_i64()).unwrap_or(hours);
    format!("<b>{}</b> errors / {}h", commas(total), h)
}

fn metrics_total(m: &Value, hours: i64) -> String {
    let total = vi64(m, "total");
    let users = vi64(m, "users");
    let h = m.get("hours").and_then(|x| x.as_i64()).unwrap_or(hours);
    format!(
        "<b>{}</b> events · {} users / {}h",
        commas(total),
        commas(users),
        h
    )
}

fn errors_thead(issues: bool, f: &Filters) -> String {
    let freq = if f.sort.as_deref() == Some("frequent") {
        " ▾"
    } else {
        ""
    };
    let recent = if f.sort.as_deref() == Some("recent") {
        " ▾"
    } else {
        ""
    };
    if issues {
        format!(
            "<tr><th></th><th>Issue</th><th class=\"cnt sortable\" id=\"sort-freq\">events{freq}</th><th class=\"cnt\">users</th><th>type</th><th>first</th><th class=\"sortable\" id=\"sort-recent\">last seen{recent}</th><th></th></tr>"
        )
    } else {
        "<tr><th></th><th>Title</th><th>type</th><th>source</th><th>when</th></tr>".to_string()
    }
}

fn metrics_thead(issues: bool, f: &Filters) -> String {
    let freq = if f.sort.as_deref() == Some("frequent") {
        " ▾"
    } else {
        ""
    };
    let recent = if f.sort.as_deref() == Some("recent") {
        " ▾"
    } else {
        ""
    };
    if issues {
        format!(
            "<tr><th></th><th>Event</th><th class=\"cnt sortable\" id=\"sort-freq\">count{freq}</th><th class=\"cnt\">users</th><th class=\"sortable\" id=\"sort-recent\">last seen{recent}</th></tr>"
        )
    } else {
        "<tr><th></th><th>Event</th><th>type</th><th>when</th></tr>".to_string()
    }
}

fn render_rows(data: &Value, issues: bool, metrics: bool) -> String {
    let items = match data.get("items").and_then(|i| i.as_array()) {
        Some(a) => a,
        None => return String::new(),
    };
    items
        .iter()
        .map(|it| row_html(it, issues, metrics))
        .collect::<Vec<_>>()
        .join("")
}

fn row_html(it: &Value, issues: bool, metrics: bool) -> String {
    if metrics {
        let title = esc(&strip_tags(&{
            let t = vstr(it, "title");
            if t.is_empty() {
                "(event)".to_string()
            } else {
                t
            }
        }));
        if issues {
            let fp = esc(&vstr(it, "fingerprint"));
            let count = commas(vi64(it, "count"));
            let users = commas(vi64(it, "users"));
            let last = vstr(it, "last_seen");
            return format!(
                "<tr class=\"ev\" data-fp=\"{fp}\"><td><span class=\"lvl info\"></span></td>\n      <td><div class=\"title\">{title}</div></td><td class=\"cnt\">{count}</td>\n      <td class=\"cnt\">{users}</td>\n      <td class=\"when\" title=\"{lt}\">{ltshort}</td></tr>",
                lt = esc(&last), ltshort = esc(&short_time(&last))
            );
        }
        let id = vi64(it, "id");
        let kind = esc(&vstr(it, "kind"));
        let recv = vstr(it, "received_at");
        return format!(
            "<tr class=\"ev\" data-id=\"{id}\"><td><span class=\"lvl info\"></span></td>\n      <td><div class=\"title\">{title}</div></td><td class=\"tag\">{kind}</td>\n      <td class=\"when\" title=\"{rt}\">{rtshort}</td></tr>",
            rt = esc(&recv), rtshort = esc(&short_time(&recv))
        );
    }
    let level = vstr(it, "level");
    let lc = lvl_class(&level);
    let title = esc(&strip_tags(&{
        let t = vstr(it, "title");
        if t.is_empty() {
            "(no message)".to_string()
        } else {
            t
        }
    }));
    if issues {
        let fp = esc(&vstr(it, "fingerprint"));
        let status = {
            let s = vstr(it, "status");
            if s.is_empty() {
                "unresolved".to_string()
            } else {
                s
            }
        };
        let sb = if status != "unresolved" {
            format!("<span class=\"sbadge {status}\">{status}</span> ")
        } else {
            String::new()
        };
        let acts = if status == "unresolved" {
            format!(
                "<button class=\"act\" data-fp=\"{fp}\" data-st=\"resolved\" title=\"resolve\">✓</button><button class=\"act\" data-fp=\"{fp}\" data-st=\"ignored\" title=\"ignore\">⊘</button>"
            )
        } else {
            format!("<button class=\"act\" data-fp=\"{fp}\" data-st=\"unresolved\" title=\"unresolve\">↺</button>")
        };
        let count = commas(vi64(it, "count"));
        let users = commas(vi64(it, "users"));
        let kind = esc(&vstr(it, "kind"));
        let first = vstr(it, "first_seen");
        let last = vstr(it, "last_seen");
        return format!(
            "<tr class=\"ev {status}\" data-fp=\"{fp}\"><td><span class=\"lvl {lc}\"></span></td>\n    <td><div class=\"title\">{sb}{title}</div><div class=\"fp\">{fp}</div></td>\n    <td class=\"cnt\">{count}</td><td class=\"cnt\">{users}</td>\n    <td class=\"tag\">{kind}</td>\n    <td class=\"when\" title=\"first seen {fst}\">{fshort}</td>\n    <td class=\"when\" title=\"{lt}\">{lshort}</td>\n    <td class=\"acts\">{acts}</td></tr>",
            fst = esc(&first), fshort = esc(&short_time(&first)),
            lt = esc(&last), lshort = esc(&short_time(&last))
        );
    }
    let id = vi64(it, "id");
    let kind = esc(&vstr(it, "kind"));
    let source = esc(&vstr(it, "source"));
    let recv = vstr(it, "received_at");
    format!(
        "<tr class=\"ev\" data-id=\"{id}\"><td><span class=\"lvl {lc}\"></span></td>\n    <td><div class=\"title\">{title}</div></td><td class=\"tag\">{kind}</td>\n    <td class=\"tag\">{source}</td><td class=\"when\" title=\"{rt}\">{rtshort}</td></tr>",
        rt = esc(&recv), rtshort = esc(&short_time(&recv))
    )
}

fn issue_head(meta: &Value, fp: &str) -> (String, String) {
    let level = vstr(meta, "level");
    let kind = vstr(meta, "kind");
    let badge_txt = if !level.is_empty() {
        level.clone()
    } else {
        kind.clone()
    };
    let title = {
        let t = vstr(meta, "title");
        if !t.is_empty() {
            strip_tags(&t)
        } else {
            fp.to_string()
        }
    };
    let count = commas(vi64(meta, "count"));
    let users = commas(vi64(meta, "users"));
    let status = {
        let s = vstr(meta, "status");
        if s.is_empty() {
            "unresolved".to_string()
        } else {
            s
        }
    };
    let status_block = if status == "unresolved" {
        "<button class=\"act2\" data-st=\"resolved\">✓ resolve</button><button class=\"act2\" data-st=\"ignored\">⊘ ignore</button>".to_string()
    } else {
        format!("<span class=\"sbadge {s}\">{s}</span><button class=\"act2\" data-st=\"unresolved\">↺ unresolve</button>", s = esc(&status))
    };
    let assignee = vstr(meta, "assignee");
    let assignee_block = if !assignee.is_empty() {
        format!(
            "<span class=\"chip\" title=\"assignee\">@<b>{}</b></span>",
            esc(&assignee)
        )
    } else {
        "<span class=\"chip\" style=\"opacity:.55\" title=\"assignee\">unassigned</span>"
            .to_string()
    };
    let first = short_time(&vstr(meta, "first_seen"));
    let last = short_time(&vstr(meta, "last_seen"));
    let imeta = format!(
        "<span class=\"badge {bc}\">{badge}</span>\n    <span class=\"chip\"><b>{count}</b> events</span>\n    <span class=\"chip\"><b>{users}</b> users</span>\n    <span class=\"chip\">first <b>{first}</b></span>\n    <span class=\"chip\">last <b>{last}</b></span>\n    {status_block}\n    {assignee_block}",
        bc = lvl_class(&level), badge = esc(&badge_txt)
    );
    (esc(&title), imeta)
}

fn col(v: f64) -> &'static str {
    if v >= 99.0 {
        "var(--ok)"
    } else if v >= 95.0 {
        "var(--warn)"
    } else {
        "var(--err)"
    }
}

fn render_health(h: &Value) -> String {
    let f = |k: &str| h.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let crash_free = f("crash_free_rate");
    let crash_free_users = f("crash_free_users_rate");
    let healthy = f("healthy_rate");
    let total = vi64(h, "total");
    let total_users = vi64(h, "total_users");
    let crashed = vi64(h, "crashed");
    let empty = vec![];
    let by_status = h
        .get("by_status")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty);
    let sb: String = by_status.iter().map(|p| {
        let k = p.get(0).and_then(|x| x.as_str()).unwrap_or("");
        let c = p.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
        let cls = if ["crashed", "abnormal", "unhandled", "errored"].contains(&k) { "error" }
            else if k == "exited" || k == "ok" { "info" } else { "none" };
        format!("<span class=\"chip\"><span class=\"lvl {cls}\" style=\"display:inline-block;margin:0 5px 0 0\"></span>{} <b>{}</b></span>", esc(k), c)
    }).collect();
    let series = h.get("series").and_then(|x| x.as_array()).unwrap_or(&empty);
    let mx = series
        .iter()
        .filter_map(|p| p.get(1).and_then(|x| x.as_i64()))
        .max()
        .unwrap_or(1)
        .max(1);
    let bars: String = series
        .iter()
        .map(|p| {
            let b = p.get(0).and_then(|x| x.as_str()).unwrap_or("");
            let c = p.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
            format!(
                "<div class=\"bar\" style=\"height:{}%\" title=\"{} — {}\"></div>",
                (c as f64 / mx as f64 * 100.0).round() as i64,
                esc(b),
                c
            )
        })
        .collect();
    let by_release = h
        .get("by_release")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty);
    let rel_rows: String = if by_release.is_empty() {
        "<tr><td colspan=3 class=\"empty\">no sessions</td></tr>".to_string()
    } else {
        by_release.iter().map(|r| {
            let rel = r.get("release").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).unwrap_or("(none)");
            let sess = r.get("sessions").and_then(|x| x.as_i64()).unwrap_or(0);
            let cf = r.get("crash_free").and_then(|x| x.as_f64()).unwrap_or(0.0);
            format!("<tr><td>{}</td><td class=\"cnt\">{}</td><td class=\"cnt\" style=\"color:{}\">{:.2}%</td></tr>", esc(rel), commas(sess), col(cf), cf)
        }).collect()
    };
    format!(
        "<div class=\"stats\" style=\"border:0;padding:0 0 14px;margin:0\">\n      <div class=\"card\"><div class=\"lab\">crash-free sessions</div><div class=\"big\" style=\"color:{cfc}\">{cf:.2}%</div></div>\n      <div class=\"card\"><div class=\"lab\">crash-free users</div><div class=\"big\" style=\"color:{cfuc}\">{cfu:.2}%</div><div class=\"lab\">({tu} users)</div></div>\n      <div class=\"card\"><div class=\"lab\">healthy</div><div class=\"big\" style=\"color:{hc}\">{hr:.1}%</div></div>\n      <div class=\"card\"><div class=\"lab\">sessions</div><div class=\"big\">{tot}</div></div>\n      <div class=\"card\"><div class=\"lab\">crashed</div><div class=\"big\" style=\"color:var(--err)\">{crashed}</div></div>\n      <div class=\"card\" style=\"flex:1;min-width:220px\"><div class=\"lab\">by status</div><div class=\"chips\">{sb}</div></div></div>\n    <div class=\"chart\" style=\"height:60px;margin:0 0 18px\">{bars}</div>\n    <h3 class=\"sec\">by release</h3>\n    <table><thead><tr><th>release</th><th class=\"cnt\">sessions</th><th class=\"cnt\">crash-free</th></tr></thead><tbody>{rel_rows}</tbody></table>",
        cfc = col(crash_free), cf = crash_free,
        cfuc = col(crash_free_users), cfu = crash_free_users, tu = commas(total_users),
        hc = col(healthy), hr = healthy, tot = commas(total)
    )
}

fn render_flags(f: &Value) -> String {
    let config = f.get("config").cloned().unwrap_or(Value::Null);
    let flags = config
        .get("flags")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default();
    let variants = config
        .get("variants")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default();
    let empty = vec![];
    let observed = f
        .get("observed")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty);
    let obs: std::collections::HashMap<String, i64> = observed
        .iter()
        .filter_map(|p| Some((p.get(0)?.as_str()?.to_string(), p.get(1)?.as_i64()?)))
        .collect();
    let mut names: Vec<&String> = flags.keys().collect();
    names.sort();
    let source_url = vstr(f, "source_url");
    let rows: String = names.iter().map(|n| {
        let on = flags.get(*n).and_then(|x| x.as_bool()).unwrap_or(false);
        let variant = variants.get(*n).and_then(|x| x.get("name")).and_then(|x| x.as_str()).unwrap_or("");
        let obs_txt = obs.get(*n).map(|c| commas(*c)).unwrap_or_else(|| "—".to_string());
        format!(
            "<tr><td><span class=\"lvl {oc}\"></span> {name}</td>\n      <td>{state}</td>\n      <td class=\"tag\">{variant}</td><td class=\"cnt\">{obs}</td></tr>",
            oc = if on { "ok" } else { "none" }, name = esc(n),
            state = if on { "<span class=\"sbadge resolved\">on</span>" } else { "<span class=\"sbadge ignored\">off</span>" },
            variant = esc(variant), obs = obs_txt
        )
    }).collect();
    let body = if rows.is_empty() {
        "<tr><td colspan=4 class=\"empty\">flag service unreachable</td></tr>".to_string()
    } else {
        rows
    };
    format!(
        "<div class=\"when\" style=\"margin-bottom:10px\">{n} flags · live config from <b>{url}</b> · \"observed\" = times a flag appeared in event data (contexts.flags)</div>\n    <table><thead><tr><th>flag</th><th>state</th><th>variant</th><th class=\"cnt\">observed</th></tr></thead><tbody>{body}</tbody></table>",
        n = names.len(), url = esc(&source_url)
    )
}

fn render_session(s: &Value, session_id: &str) -> String {
    if s.get("user").map(|u| u.is_null()).unwrap_or(true) {
        return "<a class=\"back\" id=\"sback\">← back</a><div class=\"empty\">no session for this event (no user/app-launch id)</div>".to_string();
    }
    let user = vstr(s, "user");
    let total = vi64(s, "total");
    let errors = vi64(s, "errors");
    let first = vstr(s, "first");
    let last = vstr(s, "last");
    let dur = if !first.is_empty() && !last.is_empty() {
        let parse = |t: &str| {
            chrono::DateTime::parse_from_rfc3339(t)
                .ok()
                .map(|d| d.timestamp())
        };
        match (parse(&first), parse(&last)) {
            (Some(a), Some(b)) => (b - a).max(0),
            _ => 0,
        }
    } else {
        0
    };
    let fmt_dur = |d: i64| -> String {
        if d < 60 {
            format!("{d}s")
        } else if d < 3600 {
            format!("{}m", d / 60)
        } else {
            format!("{}h {}m", d / 3600, (d % 3600) / 60)
        }
    };
    let empty = vec![];
    let by_level = s
        .get("by_level")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty);
    let lv: String = by_level.iter().map(|p| {
        let k = p.get(0).and_then(|x| x.as_str()).unwrap_or("");
        let c = p.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
        format!("<span class=\"chip\"><span class=\"lvl {}\" style=\"display:inline-block;margin:0 5px 0 0\"></span>{} <b>{}</b></span>", lvl_class(k), esc(k), c)
    }).collect();
    let events = s.get("events").and_then(|x| x.as_array()).unwrap_or(&empty);
    let tl: String = events.iter().map(|e| {
        let id = vi64(e, "id");
        let level = vstr(e, "level");
        let kind = vstr(e, "kind");
        let recv = vstr(e, "received_at");
        let title = {
            let t = vstr(e, "title");
            if t.is_empty() { kind.clone() } else { t }
        };
        let cur = if id.to_string() == session_id { "cur" } else { "" };
        let tag = if !level.is_empty() { level.clone() } else { kind.clone() };
        format!(
            "<div class=\"tl {cur}\" data-id=\"{id}\"><span class=\"lvl {lc}\"></span>\n      <span class=\"tlw\" title=\"{rt}\">{rtshort}</span>\n      <span class=\"tltag\">{tag}</span>\n      <span class=\"tlt\">{title}</span></div>",
            lc = lvl_class(&level), rt = esc(&recv), rtshort = esc(&short_time(&recv)),
            tag = esc(&tag), title = esc(&strip_tags(&title))
        )
    }).collect();
    let app_start = vstr(s, "app_start");
    let started = {
        let src = if !app_start.is_empty() {
            &app_start
        } else {
            &first
        };
        let head: String = src.chars().take(16).collect();
        head.replace('T', " ")
    };
    let tl_block = if tl.is_empty() {
        "<div class=\"empty\">no events</div>".to_string()
    } else {
        tl
    };
    format!(
        "<a class=\"back\" id=\"sback\">← back</a>\n    <h3 class=\"sec\" style=\"margin-top:10px\">session timeline</h3>\n    <div class=\"stats\" style=\"border:0;padding:0 0 12px;margin:0\">\n      <div class=\"card\" style=\"min-width:200px\"><div class=\"lab\">user</div><div class=\"kv\" style=\"background:0;border:0;padding:4px 0\"><div class=\"v\">{user}</div></div></div>\n      <div class=\"card\"><div class=\"lab\">events</div><div class=\"big\">{total}</div></div>\n      <div class=\"card\"><div class=\"lab\">errors</div><div class=\"big\" style=\"color:var(--err)\">{errors}</div></div>\n      <div class=\"card\"><div class=\"lab\">duration</div><div class=\"big\">{dur}</div></div>\n      <div class=\"card\" style=\"flex:1;min-width:200px\"><div class=\"lab\">started {started}</div><div class=\"chips\">{lv}</div></div>\n    </div>\n    <div class=\"timeline\">{tl}</div>",
        user = esc(&user), total = commas(total), errors = commas(errors), dur = fmt_dur(dur),
        started = esc(&started), tl = tl_block
    )
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = urldecode(it.next().unwrap_or(""));
            let v = urldecode(it.next().unwrap_or(""));
            (k, v)
        })
        .collect()
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let h = hex(bytes[i + 1]);
                let l = hex(bytes[i + 2]);
                if let (Some(h), Some(l)) = (h, l) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
