mod component;
mod query;
mod rows;

pub use component::PlacesComponent;
pub use rows::{
    CategoryTarget, PlaceListFilters, PlaceOrderBy, PlaceRow, PlaceStatusRow, PoiRow, ReportRow,
    UserInteraction,
};

#[cfg(test)]
use query::{
    build_live_user_count_order, build_order_by, build_where, destinations_order_prefix, Bind,
};

#[cfg(test)]
mod wire_tests {
    use super::*;

    fn sample() -> PlaceRow {
        PlaceRow {
            id: "uuid-1".to_string(),
            title: Some("t".to_string()),
            description: None,
            image: None,
            owner: None,
            positions: vec!["1,2".to_string()],
            base_position: "1,2".to_string(),
            contact_name: None,
            contact_email: None,
            content_rating: None,
            disabled: false,
            disabled_at: None,
            disabled_reason: None,
            created_at: None,
            updated_at: None,
            favorites: 0,
            likes: 0,
            dislikes: 0,
            categories: vec![],
            tags: vec![],
            highlighted: false,
            highlighted_image: None,
            ranking: None,
            sdk: None,
            creator_address: None,
            world_id: None,
            deployed_at: None,
            world: false,
            world_name: None,
            is_private: false,
            user_favorite: false,
            user_like: false,
            user_dislike: false,
            user_count: Some(3),
            user_visits: 0,
            like_rate: None,
            like_score: None,
            live: None,
            connected_addresses: None,
            realms_detail: None,
        }
    }

    #[test]
    fn base_place_omits_destination_only_fields() {
        let v = serde_json::to_value(sample()).unwrap();
        let obj = v.as_object().unwrap();
        for absent in [
            "featured",
            "featured_image",
            "live",
            "connected_addresses",
            "realms_detail",
        ] {
            assert!(
                !obj.contains_key(absent),
                "{absent} must be omitted on base Place"
            );
        }

        assert!(obj.contains_key("highlighted"));
    }

    #[test]
    fn base_place_carries_read_path_fields() {
        let v = serde_json::to_value(sample()).unwrap();
        let obj = v.as_object().unwrap();
        for key in [
            "is_private",
            "highlighted_image",
            "sdk",
            "tags",
            "highlighted",
            "user_favorite",
            "user_like",
            "user_dislike",
            "user_count",
            "user_visits",
            "like_rate",
            "like_score",
            "creator_address",
            "world_name",
            "base_position",
            "positions",
        ] {
            assert!(obj.contains_key(key), "{key} must be present on base Place");
        }

        assert!(obj["is_private"].is_boolean());
        assert!(obj["tags"].is_array());
    }

    #[test]
    fn realms_detail_gated_for_places() {
        let mut row = sample();
        row.apply_realms_detail(false);
        let v = serde_json::to_value(&row).unwrap();
        assert!(
            !v.as_object().unwrap().contains_key("realms_detail"),
            "realms_detail must be omitted when with_realms_detail is off"
        );

        row.apply_realms_detail(true);
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["realms_detail"], serde_json::json!([]));
    }

    #[test]
    fn realms_detail_skipped_for_worlds() {
        let mut row = sample();
        row.world = true;
        row.apply_realms_detail(true);
        let v = serde_json::to_value(&row).unwrap();
        assert!(
            !v.as_object().unwrap().contains_key("realms_detail"),
            "realms_detail must stay omitted for worlds"
        );
    }

    #[test]
    fn enriched_destination_emits_fields() {
        let mut row = sample();
        row.connected_addresses = Some(vec!["0xaaa".to_string(), "0xbbb".to_string()]);
        row.live = Some(true);
        row.realms_detail = Some(vec![]);

        let v = serde_json::to_value(&row).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(
            obj["connected_addresses"],
            serde_json::json!(["0xaaa", "0xbbb"])
        );
        assert_eq!(obj["live"], serde_json::json!(true));
        assert_eq!(obj["realms_detail"], serde_json::json!([]));
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn names_filter_clause() {
        let f = PlaceListFilters {
            names: vec!["Foo.DCL.eth".to_string()],
            ..Default::default()
        };
        let (where_clause, binds) = build_where(&f);
        assert!(where_clause.contains("lower(raw->>'world_name') = ANY"));
        match binds.last().unwrap() {
            Bind::TextArray(v) => assert_eq!(v, &vec!["foo.dcl.eth".to_string()]),
            _ => panic!("expected names text array bind"),
        }
    }

    #[test]
    fn owner_operated_positions_clause() {
        let f = PlaceListFilters {
            operated_positions: vec!["10,20".to_string()],
            ..Default::default()
        };
        let (where_clause, binds) = build_where(&f);
        assert!(where_clause.contains("raw->'positions' ?|"));
        match binds.last().unwrap() {
            Bind::TextArray(v) => assert_eq!(v, &vec!["10,20".to_string()]),
            _ => panic!("expected positions text array bind"),
        }
    }

    #[test]
    fn owner_filtered_with_no_positions_forces_empty() {
        let f = PlaceListFilters {
            owner_filtered: true,
            operated_positions: vec![],
            ..Default::default()
        };
        let (where_clause, _) = build_where(&f);

        assert!(where_clause.contains("AND FALSE"));
        assert!(!where_clause.contains("raw->'positions' ?|"));

        let g = PlaceListFilters::default();
        let (where_g, _) = build_where(&g);
        assert!(!where_g.contains("AND FALSE"));
    }

    #[test]
    fn sdk_filter_clause() {
        let f = PlaceListFilters {
            sdk: Some("7".to_string()),
            ..Default::default()
        };
        let (where_clause, _) = build_where(&f);
        assert!(where_clause.contains("raw->>'sdk' ="));
        assert!(where_clause.contains("raw->>'sdk' LIKE"));
        assert!(!where_clause.contains("raw->>'sdk' IS NULL"));

        let f6 = PlaceListFilters {
            sdk: Some("6".to_string()),
            ..Default::default()
        };
        let (where6, _) = build_where(&f6);
        assert!(where6.contains("raw->>'sdk' IS NULL"));
    }

    #[test]
    fn creator_address_clause() {
        let f = PlaceListFilters {
            creator_address: Some("0xABC".to_string()),
            ..Default::default()
        };
        let (where_clause, binds) = build_where(&f);
        assert!(where_clause.contains("LOWER(creator_address) ="));
        match binds.last().unwrap() {
            Bind::Text(s) => assert_eq!(s, "0xabc"),
            _ => panic!("expected creator text bind"),
        }
    }
}

#[cfg(test)]
mod most_active_order_tests {
    use super::*;

    #[test]
    fn most_active_with_place_counts_emits_live_desc_prefix() {
        let f = PlaceListFilters {
            order_by: PlaceOrderBy::MostActive,
            place_user_counts: vec![("10,20".to_string(), 42), ("0,0".to_string(), 7)],
            ..Default::default()
        };
        let (prefix, binds) = build_live_user_count_order(&f, 1);
        assert!(
            !prefix.is_empty(),
            "most_active + realtime counts must emit a live ordering prefix"
        );
        assert!(
            prefix.contains("CASE base_position"),
            "places must be matched on base_position: {prefix}"
        );
        assert!(
            prefix.contains("::int DESC"),
            "live user count must sort DESC: {prefix}"
        );
        assert!(
            prefix.trim_end().ends_with("DESC,"),
            "live prefix must be a leading ORDER BY term (trailing comma): {prefix}"
        );
        assert_eq!(binds.len(), 4, "two (pos,count) pairs => four binds");
        match &binds[0] {
            Bind::Text(s) => assert_eq!(s, "10,20"),
            _ => panic!("expected position text bind"),
        }
        match &binds[1] {
            Bind::Int(n) => assert_eq!(*n, 42),
            _ => panic!("expected count int bind"),
        }
    }

    #[test]
    fn most_active_with_world_counts_matches_lowercased_world_name() {
        let f = PlaceListFilters {
            order_by: PlaceOrderBy::MostActive,
            world_user_counts: vec![("MyWorld.DCL.eth".to_string(), 99)],
            ..Default::default()
        };
        let (prefix, binds) = build_live_user_count_order(&f, 1);
        assert!(
            prefix.contains("lower(raw->>'world_name')"),
            "worlds must be matched on lower(world_name): {prefix}"
        );
        assert!(
            prefix.contains("::int DESC"),
            "world live count must sort DESC"
        );
        match &binds[0] {
            Bind::Text(s) => assert_eq!(s, "myworld.dcl.eth"),
            _ => panic!("expected lower-cased world name bind"),
        }
        match &binds[1] {
            Bind::Int(n) => assert_eq!(*n, 99),
            _ => panic!("expected count int bind"),
        }
    }

    #[test]
    fn most_active_without_counts_falls_back_to_stored_order() {
        let f = PlaceListFilters {
            order_by: PlaceOrderBy::MostActive,
            ..Default::default()
        };
        let (prefix, binds) = build_live_user_count_order(&f, 1);
        assert!(
            prefix.is_empty(),
            "no realtime data => no live ordering prefix (degrade to stored order)"
        );
        assert!(binds.is_empty(), "no live binds when counts are empty");
        assert_eq!(
            f.order_by.column(),
            "COALESCE(NULLIF(raw->>'user_count','')::int, 0)"
        );
    }

    #[test]
    fn non_most_active_never_emits_live_prefix() {
        for ob in [
            PlaceOrderBy::LikeScore,
            PlaceOrderBy::UpdatedAt,
            PlaceOrderBy::CreatedAt,
            PlaceOrderBy::UserVisits,
        ] {
            let f = PlaceListFilters {
                order_by: ob,
                place_user_counts: vec![("1,1".to_string(), 5)],
                world_user_counts: vec![("w.dcl.eth".to_string(), 3)],
                ..Default::default()
            };
            let (prefix, binds) = build_live_user_count_order(&f, 1);
            assert!(
                prefix.is_empty(),
                "{ob:?} must not inject a live ordering prefix"
            );
            assert!(binds.is_empty(), "{ob:?} must not inject live binds");
        }
    }

    #[test]
    fn param_indices_are_sequential_across_place_and_world_branches() {
        let f = PlaceListFilters {
            order_by: PlaceOrderBy::MostActive,
            place_user_counts: vec![("10,20".to_string(), 42)],
            world_user_counts: vec![("w.dcl.eth".to_string(), 9)],
            ..Default::default()
        };
        let start = 5;
        let (prefix, binds) = build_live_user_count_order(&f, start);
        assert!(
            prefix.contains("CASE base_position WHEN $5 THEN $6"),
            "place branch must start at start_idx: {prefix}"
        );
        assert!(
            prefix.contains("CASE lower(raw->>'world_name') WHEN $7 THEN $8"),
            "world branch must continue after place branch: {prefix}"
        );
        assert!(
            prefix.contains("CASE WHEN COALESCE((raw->>'world')::bool, false) THEN"),
            "row is routed to world vs place arm by the world flag: {prefix}"
        );
        assert_eq!(binds.len(), 4);
    }

    #[test]
    fn absent_branch_defaults_to_zero() {
        let f = PlaceListFilters {
            order_by: PlaceOrderBy::MostActive,
            place_user_counts: vec![("10,20".to_string(), 42)],
            ..Default::default()
        };
        let (prefix, _) = build_live_user_count_order(&f, 1);
        assert!(
            prefix.contains("THEN 0 ELSE"),
            "empty worlds arm must default to 0: {prefix}"
        );
    }
}

#[cfg(test)]
mod destinations_order_tests {
    use super::*;

    #[test]
    fn destinations_mode_prefixes_highlighted_then_ranking() {
        let f = PlaceListFilters {
            destinations_mode: true,
            ..Default::default()
        };
        let prefix = destinations_order_prefix(&f);
        assert!(!prefix.is_empty(), "destinations mode must emit a prefix");
        let hi = prefix.find("highlighted DESC").expect("highlighted term");
        let rk = prefix.find("ranking").expect("ranking term");
        assert!(hi < rk, "highlighted must precede ranking: {prefix}");
        assert!(
            prefix.contains("NULLIF(raw->>'ranking','')::float8 DESC NULLS LAST"),
            "ranking must be NULLIF(raw->>'ranking','')::float8 DESC NULLS LAST: {prefix}"
        );
        assert!(
            prefix.ends_with(", "),
            "prefix must carry trailing comma: {prefix}"
        );
    }

    #[test]
    fn places_mode_has_no_highlighted_ranking_prefix() {
        let f = PlaceListFilters {
            destinations_mode: false,
            ..Default::default()
        };
        assert_eq!(destinations_order_prefix(&f), "");
        assert_eq!(destinations_order_prefix(&PlaceListFilters::default()), "");
    }

    #[test]
    fn order_by_puts_destinations_prefix_before_live_rank_and_column() {
        let dest = destinations_order_prefix(&PlaceListFilters {
            destinations_mode: true,
            ..Default::default()
        });
        let live = "(CASE WHEN true THEN 1 ELSE 0 END)::int DESC, ";
        let rank = "ts_rank_cd(x, y, 32) DESC, ";
        let clause = build_order_by(
            dest,
            live,
            rank,
            "NULLIF(raw->>'like_score','')::float8",
            "DESC",
        );
        let p_hi = clause.find("highlighted DESC").expect("highlighted");
        let p_live = clause.find("::int DESC").expect("live");
        let p_rank = clause.find("ts_rank_cd").expect("rank");
        let p_col = clause.find("like_score").expect("order column");
        assert!(
            p_hi < p_live,
            "destinations prefix must precede live: {clause}"
        );
        assert!(p_live < p_rank, "live must precede search rank: {clause}");
        assert!(
            p_rank < p_col,
            "search rank must precede order column: {clause}"
        );
        assert!(
            clause.trim_end().ends_with("deployed_at DESC"),
            "deployed_at is the final tiebreaker: {clause}"
        );
    }

    #[test]
    fn order_by_without_destinations_prefix_starts_with_column() {
        let clause = build_order_by("", "", "", "NULLIF(raw->>'like_score','')::float8", "DESC");
        assert!(
            !clause.contains("highlighted DESC"),
            "places clause must not carry the highlighted prefix: {clause}"
        );
        assert!(
            clause.starts_with("NULLIF(raw->>'like_score','')::float8 DESC NULLS LAST"),
            "order column must lead: {clause}"
        );
        assert!(clause.ends_with("deployed_at DESC"), "{clause}");
    }
}
