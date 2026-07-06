use super::*;

pub(super) fn controls(va: &ViewerAdmin, which: &str) -> String {
    if !va.enabled {
        return String::new();
    }
    let mut b = String::new();
    b.push_str("<section><div class=\"shead\"><h3>Operator controls</h3><span class=\"c\">privileged actions — confirm before each</span></div>");

    if !va.is_admin {
        b.push_str("<div class=\"ctlcard\"><div class=\"ctlh\">Sign in</div>");
        b.push_str("<div class=\"ctlbody\"><p class=\"ctldesc\">Write controls are configured for this realm. Authenticate with an allowlisted wallet to enable them.</p>");
        b.push_str("<button class=\"btn\" id=\"admin-signin\">Sign in with wallet</button>");
        b.push_str("<span id=\"admin-who\" class=\"who\"></span>");
        b.push_str("<div class=\"ctl-result\" id=\"signin-result\"></div></div></div>");
        b.push_str("</section>");
        return b;
    }

    let addr = va.addr.clone().unwrap_or_default();
    b.push_str(&format!(
        "<div class=\"ctlcard\"><div class=\"ctlh\">Session</div><div class=\"ctlbody\"><p class=\"ctldesc\">Signed in as <span class=\"mono who\" id=\"admin-who\">{}</span></p><button class=\"btn ghost\" id=\"admin-signout\">Sign out</button></div></div>",
        esc(&addr)
    ));

    let want = |key: &str| which == "all" || which == key;

    if want("content") {
        b.push_str(&ctl_card(
            "Content",
            "Drop the in-process deployments cache; the next read repopulates from the database.",
            &[ctl_button(
                "Flush deployments cache",
                "/admin/api/content/flush-cache",
                "Flush the deployments cache?",
            )],
        ));
    }

    if want("telemetry") && has_service("telemetry") {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/issue-state\" data-confirm=\"Update this issue's state?\">");
        inner.push_str("<label>Fingerprint<input name=\"fingerprint\" placeholder=\"issue fingerprint\" required></label>");
        inner.push_str("<label>Status<select name=\"status\"><option value=\"resolve\">resolve</option><option value=\"ignore\">ignore</option><option value=\"unresolve\">unresolve</option></select></label>");
        inner.push_str("<label>Assignee<input name=\"assignee\" placeholder=\"address / handle (optional)\"></label>");
        inner.push_str("<label>Note<input name=\"note\" placeholder=\"optional note\"></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set issue state</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/sql\" data-result=\"#sql-out\">");
        inner.push_str("<label>Ad-hoc SQL <span class=\"hint\">read-only, enforced downstream</span><textarea name=\"sql\" rows=\"3\" placeholder=\"select ...\" required></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Run query</button>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"sql-out\"></pre></form>");
        b.push_str(&ctl_card_raw(
            "Telemetry",
            "Triage issues and run read-only queries against the telemetry store.",
            &inner,
        ));
    }

    if want("create")
        && has_service("create")
        && env_set_any(&["AB_REGISTRY_ADMIN_TOKEN", "API_ADMIN_TOKEN"])
    {
        b.push_str(&ctl_card(
            "Create",
            "Re-ingest the asset-bundle registry or flush its build cache.",
            &[
                ctl_button(
                    "Re-ingest registry",
                    "/admin/api/create/registry-reingest",
                    "Re-ingest the AB registry?",
                ),
                ctl_button(
                    "Flush AB cache",
                    "/admin/api/create/flush-ab-cache",
                    "Flush the asset-bundle cache?",
                ),
            ],
        ));
    }

    if want("social")
        && has_service("social")
        && env_set_any(&["COMMS_MODERATOR_TOKEN", "MODERATOR_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social/user-ban\" data-confirm=\"Ban this user?\">");
        inner.push_str(
            "<label>Address<input name=\"address\" placeholder=\"0x…\" required></label>",
        );
        inner.push_str(
            "<label>Reason<input name=\"reason\" placeholder=\"reason (optional)\"></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Ban</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social/user-unban\" data-fields=\"address\" data-confirm=\"Unban this user?\">Unban</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social/user-warning\" data-fields=\"address,reason\" data-confirm=\"Warn this user?\">Warn</button>");
        inner.push_str("</div>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Social moderation",
            "Ban, unban, or warn a user via the comms gatekeeper.",
            &inner,
        ));
    }

    if want("scene-state") && has_service("scene-state") && env_set_any(&["DEBUGGING_SECRET"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/scene/reload\" data-confirm=\"Reload this scene?\">");
        inner.push_str(
            "<label>Scene<input name=\"name\" placeholder=\"scene id or name\" required></label>",
        );
        inner.push_str("<button class=\"btn\" type=\"submit\">Reload scene</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Scene state",
            "Force a reload of an authoritative SDK7 scene.",
            &inner,
        ));
    }

    if want("content") {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");

        inner.push_str(&btn_field(
            "Clear failed deployments",
            "/admin/api/content/failed-deployments/clear",
            "Clear the failed-deployments queue?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Refresh challenge",
            "/admin/api/content/challenge/refresh",
            "Refresh the content challenge?",
            "",
            false,
        ));
        inner.push_str("</div>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field(
            "Pause sync",
            "/admin/api/content/sync/pause",
            "Pause content synchronization?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Resume sync",
            "/admin/api/content/sync/resume",
            "Resume content synchronization?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Force sync",
            "/admin/api/content/sync/force",
            "Force a sync pass now?",
            "",
            false,
        ));
        inner.push_str("</div>");
        inner.push_str("<div class=\"ctl-result\"></div>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/denylist/add\" data-confirm=\"Add this entity to the content denylist?\">");
        inner.push_str("<label>Entity ID<input name=\"entity_id\" placeholder=\"bafy… / Qm…\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Denylist add</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/content/denylist/remove\" data-fields=\"entity_id\" data-confirm=\"Remove this entity from the denylist?\">Denylist remove</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/content/denylist/list\" data-fields=\"\">List denylist</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/read-only\" data-confirm=\"Change the read-only flag?\">");
        inner.push_str("<label>Read-only<select name=\"enabled\"><option value=\"true\">enable (read-only)</option><option value=\"false\">disable (writable)</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set read-only</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/accepting-users\" data-confirm=\"Change whether the realm accepts users?\">");
        inner.push_str("<label>Accepting users<select name=\"enabled\"><option value=\"true\">accepting</option><option value=\"false\">closed</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set accepting-users</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Content operations",
            "Synchronization, snapshots, denylist and realm mode for the local content core.",
            &inner,
        ));
    }

    if want("explore") && has_service("explore") && env_set_any(&["PLACES_ADMIN_AUTH_TOKEN"]) {
        let mut inner = String::new();

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/reports\" data-result=\"#places-reports-out\">");
        inner.push_str(
            "<label>Reports<span class=\"hint\">filter & list moderation reports</span></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<input name=\"status\" placeholder=\"status (optional)\"><input name=\"entity_id\" placeholder=\"entity_id (optional)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List reports</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"places-reports-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/report-resolve\" data-confirm=\"Resolve this report?\">");
        inner.push_str(
            "<label>Resolve report<input name=\"id\" placeholder=\"report id\" required></label>",
        );
        inner.push_str(
            "<label>Status<input name=\"status\" placeholder=\"resolved / dismissed\"></label>",
        );
        inner.push_str("<button class=\"btn\" type=\"submit\">Resolve report</button><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/place-disable\" data-confirm=\"Disable this place?\">");
        inner.push_str("<label>Disable place<input name=\"place_id\" placeholder=\"place id\" required></label>");
        inner.push_str("<label>Disabled<select name=\"disabled\"><option value=\"true\">disable</option><option value=\"false\">re-enable</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set place disabled</button><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/place-highlight\" data-confirm=\"Update this place's highlight?\">");
        inner.push_str("<label>Place highlight / rating<input name=\"place_id\" placeholder=\"place id\" required></label>");
        inner.push_str("<input name=\"highlighted\" placeholder=\"highlighted true/false\"><input name=\"rating\" placeholder=\"rating (for rating action)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set highlight</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/place-rating\" data-fields=\"place_id,rating\" data-confirm=\"Set this place's rating?\">Set rating</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/world-highlight\" data-confirm=\"Update this world's highlight?\">");
        inner.push_str("<label>World highlight / rating<input name=\"world_id\" placeholder=\"world id\" required></label>");
        inner.push_str("<input name=\"highlighted\" placeholder=\"highlighted true/false\"><input name=\"rating\" placeholder=\"rating (for rating action)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set highlight</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/world-rating\" data-fields=\"world_id,rating\" data-confirm=\"Set this world's rating?\">Set rating</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/poi-create\" data-confirm=\"Create / update this POI?\">");
        inner.push_str("<label>POIs<span class=\"hint\">curated points of interest</span></label>");
        inner.push_str("<input name=\"position\" placeholder=\"position e.g. 0,0\"><input name=\"entity_id\" placeholder=\"entity_id (optional)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/pois-list\" data-fields=\"\">List POIs</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create POI</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/poi-update\" data-fields=\"position,entity_id\" data-confirm=\"Update this POI?\">Update POI</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/poi-delete\" data-fields=\"position\" data-confirm=\"Delete this POI?\">Delete POI</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Places",
            "Moderation reports, place/world highlights & ratings, and curated POIs.",
            &inner,
        ));
    }

    if want("explore") && has_service("explore") && env_set_any(&["CATALYRST_EVENTS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/events/create\" data-confirm=\"Create this event?\">");
        inner.push_str(
            "<label>Create event<input name=\"name\" placeholder=\"event name\" required></label>",
        );
        inner.push_str("<input name=\"x\" placeholder=\"x\"><input name=\"y\" placeholder=\"y\"><input name=\"start_at\" placeholder=\"start_at ISO\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create event</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/events/moderate\" data-confirm=\"Moderate this event?\">");
        inner.push_str("<label>Moderate event<input name=\"event_id\" placeholder=\"event id\" required></label>");
        inner.push_str(
            "<label>Action<input name=\"approved\" placeholder=\"approved true/false\"></label>",
        );
        inner.push_str("<button class=\"btn\" type=\"submit\">Moderate event</button><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Events",
            "Create and moderate realm events.",
            &inner,
        ));
    }

    if want("explore") && has_service("explore") && env_set_any(&["CATALYRST_WORLDS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/list\" data-result=\"#worlds-list-out\">");
        inner.push_str(
            "<label>Worlds<span class=\"hint\">list & inspect world realms</span></label>",
        );
        inner.push_str("<div class=\"btnrow\"><input name=\"limit\" placeholder=\"limit\"><input name=\"offset\" placeholder=\"offset\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List worlds</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-list-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/detail\" data-result=\"#worlds-detail-out\">");
        inner.push_str("<label>World name<input name=\"world_name\" placeholder=\"name.dcl.eth\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Detail</button>");
        inner.push_str("<button class=\"btn\" type=\"button\" data-admin-action=\"/admin/api/worlds/enable\" data-fields=\"world_name\" data-confirm=\"Enable this world?\">Enable</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/disable\" data-fields=\"world_name\" data-confirm=\"Disable this world?\">Disable</button>");
        inner.push_str(
            "</div><pre class=\"ctl-result sqlout\" id=\"worlds-detail-out\"></pre></form>",
        );
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/ban-status\" data-result=\"#worlds-ban-out\">");
        inner.push_str("<label>Ban status<input name=\"world_name\" placeholder=\"name.dcl.eth\" required></label>");
        inner.push_str("<input name=\"address\" placeholder=\"0x… (optional)\"><input name=\"parcel\" placeholder=\"parcel (optional)\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Check ban status</button>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-ban-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/blocked-add\" data-confirm=\"Block this wallet from worlds?\">");
        inner.push_str(
            "<label>Blocked wallets<input name=\"wallet\" placeholder=\"0x…\" required></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Block wallet</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/blocked-remove\" data-fields=\"wallet\" data-confirm=\"Unblock this wallet?\">Unblock wallet</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/blocked-list\" data-fields=\"\">List blocked</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/access-log\" data-result=\"#worlds-log-out\">");
        inner.push_str(
            "<label>Access log<span class=\"hint\">recent world access events</span></label>",
        );
        inner.push_str("<div class=\"btnrow\"><input name=\"world\" placeholder=\"world (optional)\"><input name=\"address\" placeholder=\"address (optional)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">View access log</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-log-out\"></pre></form>");
        b.push_str(&ctl_card_raw(
            "Worlds",
            "Enable/disable worlds, manage the blocklist, and inspect ban status & access logs.",
            &inner,
        ));
    }

    if want("create")
        && has_service("create")
        && env_set_any(&["API_ADMIN_TOKEN", "AB_REGISTRY_ADMIN_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field(
            "Retry queues",
            "/admin/api/create/queues-retry",
            "Retry failed AB build jobs?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Pause queues",
            "/admin/api/create/queues-pause",
            "Pause AB build queues?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Resume queues",
            "/admin/api/create/queues-resume",
            "Resume AB build queues?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Queue status",
            "/admin/api/create/queues-status",
            "",
            "",
            false,
        ));
        inner.push_str("</div><div class=\"ctl-result\"></div>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/create/denylist-add\" data-confirm=\"Add this entity to the AB denylist?\">");
        inner.push_str("<label>AB denylist<input name=\"entity_id\" placeholder=\"entity id\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Denylist add</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/create/denylist-remove\" data-fields=\"entity_id\" data-confirm=\"Remove this entity from the AB denylist?\">Denylist remove</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "AB registry",
            "Control asset-bundle build queues and the registry denylist.",
            &inner,
        ));
    }

    if want("create")
        && has_service("create")
        && env_set_any(&["CATALYRST_CAMERA_REEL_ADMIN_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/camera-reel/image-delete\" data-confirm=\"Delete this image?\">");
        inner.push_str(
            "<label>Image<input name=\"image_id\" placeholder=\"image id\" required></label>",
        );
        inner.push_str("<label>Review action<input name=\"action\" placeholder=\"approve / reject (for review)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/camera-reel/image-review\" data-fields=\"image_id,action\" data-confirm=\"Review this image?\">Review image</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Delete image</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Camera reel",
            "Moderate in-world photos: review or delete an image.",
            &inner,
        ));
    }

    if want("create") && has_service("create") && env_set_any(&["CATALYRST_BUILDER_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/builder/item-status\" data-confirm=\"Set this item's status?\">");
        inner.push_str("<label>Item status<input name=\"collection_id\" placeholder=\"collection id\" required></label>");
        inner.push_str("<input name=\"item_id\" placeholder=\"item id\"><input name=\"status\" placeholder=\"status\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set item status</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/builder/items-status\" data-fields=\"collection_id,status\" data-confirm=\"Bulk-set every item's status in this collection?\">Bulk set status</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Builder",
            "Approve or reject collection items (single or bulk).",
            &inner,
        ));
    }

    if want("social") && has_service("social") && env_set_any(&["API_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/communities/list\" data-result=\"#communities-out\">");
        inner.push_str("<label>Communities<span class=\"hint\">list & filter</span></label>");
        inner.push_str("<div class=\"btnrow\"><input name=\"status\" placeholder=\"status\"><input name=\"owner\" placeholder=\"owner\"><input name=\"search\" placeholder=\"search\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List communities</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"communities-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/communities/suspend\" data-confirm=\"Suspend this community?\">");
        inner.push_str("<label>Suspend / unsuspend<input name=\"id\" placeholder=\"community id\" required></label>");
        inner.push_str(
            "<label>Reason<input name=\"reason\" placeholder=\"reason (optional)\"></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Suspend</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/communities/unsuspend\" data-fields=\"id\" data-confirm=\"Unsuspend this community?\">Unsuspend</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Communities",
            "List, suspend, and reinstate communities.",
            &inner,
        ));
    }

    if want("social")
        && has_service("social")
        && env_set_any(&["CATALYRST_NOTIFICATIONS_ADMIN_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/notifications/broadcast\" data-confirm=\"Broadcast this notification to all users?\">");
        inner.push_str("<label>Title<input name=\"title\" placeholder=\"notification title\" required></label>");
        inner.push_str("<label>Body<textarea name=\"body\" rows=\"2\" placeholder=\"message body\"></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Broadcast</button><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Notifications",
            "Broadcast a notification to every user.",
            &inner,
        ));
    }

    if want("social") && has_service("social") && env_set_any(&["CATALYRST_BADGES_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/badges/grant\" data-confirm=\"Grant this badge?\">");
        inner.push_str("<label>Grant / revoke badge<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str(
            "<label>Badge ID<input name=\"badge_id\" placeholder=\"badge id\" required></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Grant badge</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/badges/revoke\" data-fields=\"address,badge_id\" data-confirm=\"Revoke this badge?\">Revoke badge</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Badges",
            "Grant or revoke a profile badge for a user.",
            &inner,
        ));
    }

    if want("social-rpc")
        && has_service("social-rpc")
        && env_set_any(&["CATALYRST_SOCIAL_RPC_ADMIN_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social-rpc/presence\" data-result=\"#srpc-out\">");
        inner.push_str(
            "<label>Inspect<span class=\"hint\">presence / voice / friendships</span></label>",
        );
        inner.push_str("<input name=\"address\" placeholder=\"address (for friendships)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Presence</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/voice-calls\" data-fields=\"limit\">Voice calls</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/friendships\" data-fields=\"address,limit\">Friendships</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"srpc-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social-rpc/disconnect\" data-confirm=\"Force-disconnect this address?\">");
        inner.push_str(
            "<label>Operate<input name=\"address\" placeholder=\"0x…\" required></label>",
        );
        inner.push_str("<input name=\"presence\" placeholder=\"presence (for force-presence)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Disconnect</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/force-presence\" data-fields=\"address,presence\" data-confirm=\"Force this address's presence?\">Force presence</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/reset-settings\" data-fields=\"address\" data-confirm=\"Reset this address's social settings?\">Reset settings</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Social RPC", "Inspect presence/voice/friendships and force-disconnect or reset a user's social session.", &inner));
    }

    if want("scene-state")
        && has_service("scene-state")
        && env_set_any(&["CATALYRST_SCENE_STATE_ADMIN_TOKEN", "DEBUGGING_SECRET"])
    {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/scene-state/crdt\" data-result=\"#scenestate-out\">");
        inner.push_str(
            "<label>Scene<input name=\"scene\" placeholder=\"scene id\" required></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Inspect CRDT</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/scene-state/kick-all\" data-fields=\"scene\" data-confirm=\"Kick everyone from this scene?\">Kick all</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/scene-state/reset\" data-fields=\"scene\" data-confirm=\"Reset this scene's authoritative state? This discards all current CRDT data.\">Reset scene</button>");
        inner
            .push_str("</div><pre class=\"ctl-result sqlout\" id=\"scenestate-out\"></pre></form>");
        b.push_str(&ctl_card_raw(
            "Scene state (authoritative)",
            "Inspect the CRDT, kick all peers, or reset a scene's authoritative state.",
            &inner,
        ));
    }

    if want("data") && has_service("data") && env_set_any(&["CATALYRST_CREDITS_ADMIN_TOKEN"]) {
        let mut inner = String::new();

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/credits/season-create\" data-confirm=\"Create / update this credits season?\">");
        inner.push_str("<label>Seasons<span class=\"hint\">credits program seasons</span></label>");
        inner.push_str("<input name=\"id\" placeholder=\"id (for update/delete)\"><input name=\"name\" placeholder=\"name\"><input name=\"start_at\" placeholder=\"start_at\"><input name=\"end_at\" placeholder=\"end_at\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/seasons-list\" data-fields=\"\">List seasons</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create season</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/season-update\" data-fields=\"id,name,start_at,end_at\" data-confirm=\"Update this season?\">Update season</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/season-delete\" data-fields=\"id\" data-confirm=\"Delete this season? This is irreversible.\">Delete season</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/credits/goal-create\" data-confirm=\"Create / update this goal?\">");
        inner.push_str("<label>Goals<span class=\"hint\">weekly credit goals</span></label>");
        inner.push_str("<input name=\"id\" placeholder=\"id (for update/delete)\"><input name=\"weekId\" placeholder=\"weekId (for list)\"><input name=\"description\" placeholder=\"description\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/goals-list\" data-fields=\"weekId\">List goals</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create goal</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/goal-update\" data-fields=\"id,description\" data-confirm=\"Update this goal?\">Update goal</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/goal-delete\" data-fields=\"id\" data-confirm=\"Delete this goal? This is irreversible.\">Delete goal</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/credits/grant\" data-confirm=\"GRANT credits to this address? This mints real spendable Marketplace Credits. Confirm the address and amount are correct.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Credits grant / revoke (financial)<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str("<input name=\"amount\" placeholder=\"amount\"><input name=\"reason\" placeholder=\"reason (audited)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Grant credits</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/revoke\" data-fields=\"address,amount,reason\" data-confirm=\"REVOKE credits from this address? This removes spendable Marketplace Credits from a real user.\">Revoke credits</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/user-block\" data-fields=\"address,reason\" data-confirm=\"BLOCK this address from the credits program?\">Block user</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Credits",
            "Manage credit seasons & goals, and grant/revoke/block credits for users.",
            &inner,
        ));
    }

    if want("data") && has_service("data") && env_set_any(&["CATALYRST_PRICE_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/price/override-set\" data-confirm=\"SET a manual price override? This replaces the live market price feed for this pair and affects every price-quoting surface.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Price override (financial)<input name=\"token\" placeholder=\"token e.g. mana\" required></label>");
        inner.push_str("<input name=\"vs\" placeholder=\"vs e.g. usd\" required><input name=\"price\" placeholder=\"price\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Set override</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/price/override-delete\" data-fields=\"token,vs\" data-confirm=\"Delete this price override and restore the live feed?\">Delete override</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw(
            "Price overrides",
            "Manually override a token's spot price (replaces the live feed).",
            &inner,
        ));
    }

    if want("data") && has_service("data") && env_set_any(&["CATALYRST_RPC_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field(
            "Config",
            "/admin/api/rpc/config",
            "",
            "#rpc-out",
            false,
        ));
        inner.push_str(&btn_field(
            "List methods",
            "/admin/api/rpc/methods-list",
            "",
            "#rpc-out",
            false,
        ));
        inner.push_str(&btn_field(
            "List networks",
            "/admin/api/rpc/networks-list",
            "",
            "#rpc-out",
            false,
        ));
        inner.push_str(&btn_field(
            "Reset methods",
            "/admin/api/rpc/methods-reset",
            "Reset the RPC method allowlist to defaults?",
            "#rpc-out",
            false,
        ));
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"rpc-out\"></pre>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/rpc/methods-add\" data-confirm=\"Add this method to the allowlist?\">");
        inner.push_str(
            "<label>Method allowlist<input name=\"method\" placeholder=\"eth_…\" required></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Add method</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/rpc/methods-remove\" data-fields=\"method\" data-confirm=\"Remove this method from the allowlist?\">Remove method</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/rpc/networks-set\" data-confirm=\"Set this network's RPC upstream?\">");
        inner.push_str("<label>Networks<input name=\"network\" placeholder=\"mainnet / sepolia / …\" required></label>");
        inner.push_str("<input name=\"url\" placeholder=\"upstream url (for set)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set network</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/rpc/networks-delete\" data-fields=\"network\" data-confirm=\"Delete this network's RPC config?\">Delete network</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("EVM RPC", "Inspect config and manage the method allowlist & network upstreams of the JSON-RPC relay.", &inner));
    }

    if want("explorer-api")
        && has_service("explorer-api")
        && env_set_any(&["CATALYRST_EXPLORER_API_ADMIN_TOKEN"])
    {
        let mut inner = String::new();

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/flags-toggle\" data-confirm=\"Toggle this feature flag?\">");
        inner.push_str(
            "<label>Feature flags<input name=\"flag\" placeholder=\"flag name\" required></label>",
        );
        inner.push_str("<input name=\"enabled\" placeholder=\"enabled true/false\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Toggle flag</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/flags-reload\" data-fields=\"\" data-confirm=\"Reload feature flags?\">Reload flags</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/blocklist-add\" data-confirm=\"Add to the blocklist?\">");
        inner.push_str(
            "<label>Blocklist<input name=\"value\" placeholder=\"address / id\" required></label>",
        );
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Block</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/blocklist-remove\" data-fields=\"value\" data-confirm=\"Remove from the blocklist?\">Unblock</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/blocklist-reload\" data-fields=\"\" data-confirm=\"Reload the blocklist?\">Reload</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/config-set\" data-confirm=\"Set this config value?\">");
        inner.push_str("<label>Config<input name=\"key\" placeholder=\"key\" required></label>");
        inner.push_str("<input name=\"value\" placeholder=\"value (for set)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-list\" data-fields=\"\" data-result=\"#exapi-cfg-out\">List config</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-get\" data-fields=\"key\" data-result=\"#exapi-cfg-out\">Get config</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set config</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-delete\" data-fields=\"key\" data-confirm=\"Delete this config key?\">Delete config</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"exapi-cfg-out\"></pre></form>");

        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/challenges-list\" data-result=\"#exapi-auth-out\">");
        inner.push_str("<label>Auth challenges / identities<input name=\"id\" placeholder=\"id (for get/revoke)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"submit\">List challenges</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/challenge-get\" data-fields=\"id\" data-result=\"#exapi-auth-out\">Get challenge</button>");
        inner.push_str("<button class=\"btn\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/challenge-revoke\" data-fields=\"id\" data-confirm=\"Revoke this challenge?\">Revoke challenge</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/identities-list\" data-fields=\"\" data-result=\"#exapi-auth-out\">List identities</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/identity-revoke\" data-fields=\"id\" data-confirm=\"Revoke this identity?\">Revoke identity</button>");
        inner
            .push_str("</div><pre class=\"ctl-result sqlout\" id=\"exapi-auth-out\"></pre></form>");
        b.push_str(&ctl_card_raw(
            "Explorer API",
            "Feature flags, blocklist, runtime config and auth challenges/identities.",
            &inner,
        ));
    }

    if want("telemetry")
        && has_service("telemetry")
        && env_set_any(&["CATALYRST_TELEMETRY_ADMIN_TOKEN"])
    {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field(
            "Regroup",
            "/admin/api/telemetry/regroup",
            "Recompute issue grouping?",
            "",
            false,
        ));
        inner.push_str(&btn_field(
            "Release",
            "/admin/api/telemetry/release",
            "Mark a release event?",
            "",
            false,
        ));
        inner.push_str("</div><div class=\"ctl-result\"></div>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/quota\" data-confirm=\"Set the telemetry ingest quota?\">");
        inner.push_str(
            "<label>Quota<input name=\"limit\" placeholder=\"events/min\" required></label>",
        );
        inner.push_str("<button class=\"btn\" type=\"submit\">Set quota</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/ingest\" data-confirm=\"Inject a synthetic ingest event?\">");
        inner.push_str("<label>Manual ingest<textarea name=\"payload\" rows=\"2\" placeholder=\"event JSON\"></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Ingest</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/export\" data-result=\"#tel-export-out\">");
        inner.push_str("<label>Export / audit<input name=\"fingerprint\" placeholder=\"fingerprint (for audit)\"></label>");
        inner.push_str("<input name=\"action\" placeholder=\"action (for audit)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"submit\">Export</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/telemetry/audit\" data-fields=\"fingerprint,action,limit\" data-result=\"#tel-export-out\">Audit log</button>");
        inner
            .push_str("</div><pre class=\"ctl-result sqlout\" id=\"tel-export-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/telemetry/purge\" data-confirm=\"PURGE telemetry data? This permanently deletes stored events.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Destructive<input name=\"before\" placeholder=\"before ISO date (purge)\"></label>");
        inner.push_str("<input name=\"fingerprint\" placeholder=\"fingerprint (bulk-delete)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Purge</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/telemetry/bulk-delete\" data-fields=\"fingerprint\" data-confirm=\"BULK-DELETE every event for this fingerprint? This is irreversible.\">Bulk delete</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Telemetry operations", "Quota, manual ingest, regroup/release, export/audit, and destructive purge/bulk-delete.", &inner));
    }

    b.push_str("</section>");
    b
}

fn btn_field(label: &str, action: &str, confirm: &str, result: &str, danger: bool) -> String {
    let cls = if danger { "btn danger" } else { "btn ghost" };
    let conf = if confirm.is_empty() {
        String::new()
    } else {
        format!(" data-confirm=\"{}\"", esc(confirm))
    };
    let res = if result.is_empty() {
        String::new()
    } else {
        format!(" data-result=\"{}\"", esc(result))
    };
    format!(
        "<button class=\"{}\" type=\"button\" data-admin-action=\"{}\" data-fields=\"\"{}{}>{}</button>",
        cls,
        esc(action),
        conf,
        res,
        esc(label)
    )
}

fn ctl_card(title: &str, desc: &str, buttons: &[String]) -> String {
    let mut inner = String::from("<div class=\"btnrow\">");
    for btn in buttons {
        inner.push_str(btn);
    }
    inner.push_str("</div><div class=\"ctl-result\"></div>");
    ctl_card_raw(title, desc, &inner)
}

fn ctl_card_raw(title: &str, desc: &str, inner: &str) -> String {
    format!(
        "<div class=\"ctlcard\"><div class=\"ctlh\">{}</div><div class=\"ctlbody\"><p class=\"ctldesc\">{}</p>{}</div></div>",
        esc(title),
        esc(desc),
        inner
    )
}

fn ctl_button(label: &str, action: &str, confirm: &str) -> String {
    format!(
        "<button class=\"btn\" data-admin-action=\"{}\" data-confirm=\"{}\">{}</button>",
        esc(action),
        esc(confirm),
        esc(label)
    )
}

pub(super) fn admin_disabled_note() -> &'static str {
    "<div class=\"note\">Admin write controls are disabled — set ADMIN_ADDRESSES + SESSION_SECRET to enable them. This page is read-only.</div>"
}
