use super::query::Bind;
use super::rows::PlaceListFilters;

pub(super) const MAP_FALLBACK_IMAGE_PATH: &str = "/map.png";

pub(super) const WORLD_DEFAULT_THUMBNAIL_HASH: &str =
    "bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y";

pub(super) const UNWANTED_THUMBNAIL_HASHES: [&str; 2] = [
    "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku",
    "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n",
];

pub const PLACEHOLDER_TITLES: [&str; 8] = [
    "untitled",
    "interactive-text",
    "interactive text",
    "scene",
    "new scene",
    "sdk7 scene template",
    "empty scene",
    "empty",
];

pub const TEST_WORD_TITLE_REGEX: &str =
    "(^|[^A-Za-z0-9])[Tt][Ee][Ss][Tt]([^A-Za-z0-9]|$)|(^|[A-Za-z0-9])Test([A-Z]|[^A-Za-z0-9]|$)";

pub const PLACEHOLDER_TITLE_SUFFIX_REGEX: &str = "\\s*[0-9]+\\s*$";

pub(super) const TEMPLATE_CONTACT_NAME: &str = "sdk";

pub(super) fn placeholder_image_patterns() -> Vec<String> {
    let mut patterns = vec![format!("%{MAP_FALLBACK_IMAGE_PATH}%")];
    patterns.push(format!("%{WORLD_DEFAULT_THUMBNAIL_HASH}%"));
    patterns.extend(UNWANTED_THUMBNAIL_HASHES.iter().map(|h| format!("%{h}%")));
    patterns
}

pub(super) const ROAD_POSITIONS_TABLE: &str = "road_positions";

pub(super) fn build_content_quality_condition(
    f: &PlaceListFilters,
    road_positions: bool,
    idx: usize,
) -> Option<(String, Vec<Bind>)> {
    if !f.require_content_places && !f.require_content_worlds {
        return None;
    }
    let binds = vec![
        Bind::TextArray(placeholder_image_patterns()),
        Bind::Text(TEST_WORD_TITLE_REGEX.to_string()),
        Bind::Text(PLACEHOLDER_TITLE_SUFFIX_REGEX.to_string()),
        Bind::TextArray(PLACEHOLDER_TITLES.iter().map(|t| t.to_string()).collect()),
        Bind::Text(TEMPLATE_CONTACT_NAME.to_string()),
    ];
    let not_a_road = if road_positions && f.require_content_places {
        format!(
            " AND (world IS TRUE OR NOT EXISTS (SELECT 1 FROM {ROAD_POSITIONS_TABLE} rp \
             WHERE rp.position = base_position))"
        )
    } else {
        String::new()
    };
    let checks = format!(
        "raw->>'image' IS NOT NULL \
         AND TRIM(raw->>'image') <> '' \
         AND NOT (raw->>'image' LIKE ANY (${images}::text[])) \
         AND TRIM(COALESCE(title, '')) <> '' \
         AND title !~ ${test_word} \
         AND LOWER(TRIM(REGEXP_REPLACE(title, ${suffix}, ''))) <> ALL (${titles}::text[]) \
         AND (TRIM(COALESCE(raw->>'owner', '')) <> '' \
           OR (world IS TRUE AND TRIM(COALESCE(raw->>'creator_address', '')) <> '') \
           OR (world IS TRUE AND TRIM(COALESCE(world_name, '')) <> '') \
           OR (TRIM(COALESCE(raw->>'contact_name', '')) <> '' \
             AND LOWER(TRIM(raw->>'contact_name')) <> ${contact})){not_a_road}",
        images = idx,
        test_word = idx + 1,
        suffix = idx + 2,
        titles = idx + 3,
        contact = idx + 4,
    );
    let gate = format!("(highlighted IS TRUE OR ({checks}))");
    let clause = match (f.require_content_places, f.require_content_worlds) {
        (true, true) => gate,
        (true, false) => format!("(world IS TRUE OR {gate})"),
        (false, true) => format!("(world IS FALSE OR {gate})"),
        (false, false) => unreachable!(),
    };
    Some((clause, binds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn names_a_test_scene(title: &str) -> bool {
        Regex::new(TEST_WORD_TITLE_REGEX).unwrap().is_match(title)
    }

    fn is_placeholder_title(title: &str) -> bool {
        let stripped = Regex::new(PLACEHOLDER_TITLE_SUFFIX_REGEX)
            .unwrap()
            .replace(title, "");
        let stripped = stripped.trim().to_lowercase();
        PLACEHOLDER_TITLES.contains(&stripped.as_str())
    }

    #[test]
    fn test_word_regex_catches_test_as_a_word_of_its_own() {
        for title in [
            "Test Plaza",
            "streaming_test",
            "test-scene",
            "TEST",
            "test",
            "My test",
            "conTest",
            "TheTestScene",
            "ABTestScene1",
            "TestScene",
            "A Test",
        ] {
            assert!(names_a_test_scene(title), "{title:?} must be caught");
        }
    }

    #[test]
    fn test_word_regex_keeps_words_that_merely_contain_test() {
        for title in [
            "contest",
            "Contest",
            "Contest Arena",
            "Latest",
            "Latest News",
            "protest",
            "testament",
            "Testing Grounds",
            "Marble Observatory",
            "Amber Hollow",
        ] {
            assert!(!names_a_test_scene(title), "{title:?} must survive");
        }
    }

    #[test]
    fn test_word_regex_is_case_sensitive_on_the_camel_case_half() {
        assert!(names_a_test_scene("conTest"));
        assert!(!names_a_test_scene("contest"));
        assert!(names_a_test_scene("ABTestScene1"));
        assert!(!names_a_test_scene("abtestscene1"));
    }

    #[test]
    fn placeholder_titles_are_caught_with_their_trailing_counter() {
        for title in [
            "Untitled",
            "Untitled ",
            "Untitled 3",
            "New Scene 6",
            "Scene 5",
            "Scene",
            "scene",
            "SDK7 Scene Template",
            "Empty",
            "empty scene 12",
            "interactive-text",
            "Interactive Text",
        ] {
            assert!(is_placeholder_title(title), "{title:?} must be caught");
        }
    }

    #[test]
    fn real_names_ending_in_a_number_survive_the_suffix_strip() {
        for title in [
            "The Land 5",
            "Level 3",
            "Scene of the Crime",
            "Scenery",
            "Amber Hollow",
            "Angzaar User Shop #38",
        ] {
            assert!(!is_placeholder_title(title), "{title:?} must survive");
        }
    }

    #[test]
    fn image_patterns_cover_the_map_render_and_every_placeholder_hash() {
        let patterns = placeholder_image_patterns();
        assert_eq!(patterns.len(), 4);
        assert_eq!(patterns[0], "%/map.png%");
        assert_eq!(patterns[1], format!("%{WORLD_DEFAULT_THUMBNAIL_HASH}%"));
        for hash in UNWANTED_THUMBNAIL_HASHES {
            assert!(patterns.contains(&format!("%{hash}%")));
        }
    }

    fn feed(places: bool, worlds: bool) -> PlaceListFilters {
        PlaceListFilters {
            destinations_mode: true,
            require_content_places: places,
            require_content_worlds: worlds,
            ..Default::default()
        }
    }

    #[test]
    fn no_clause_unless_a_branch_asks_for_it() {
        assert!(build_content_quality_condition(&feed(false, false), true, 1).is_none());
    }

    #[test]
    fn both_branches_share_one_gate_with_curation_first() {
        let (clause, binds) = build_content_quality_condition(&feed(true, true), true, 3).unwrap();
        assert!(clause.starts_with("(highlighted IS TRUE OR ("), "{clause}");
        assert!(
            clause.contains("raw->>'image' LIKE ANY ($3::text[])"),
            "{clause}"
        );
        assert!(clause.contains("title !~ $4"), "{clause}");
        assert!(
            clause.contains("LOWER(TRIM(REGEXP_REPLACE(title, $5, ''))) <> ALL ($6::text[])"),
            "{clause}"
        );
        assert!(
            clause.contains("LOWER(TRIM(raw->>'contact_name')) <> $7"),
            "{clause}"
        );
        assert!(
            clause.contains(
                "NOT EXISTS (SELECT 1 FROM road_positions rp WHERE rp.position = base_position)"
            ),
            "{clause}"
        );
        assert_eq!(binds.len(), 5);
        match &binds[1] {
            Bind::Text(s) => assert_eq!(s, TEST_WORD_TITLE_REGEX),
            other => panic!("expected the test-word regex bind, got {other:?}"),
        }
        match &binds[3] {
            Bind::TextArray(v) => assert_eq!(v.len(), PLACEHOLDER_TITLES.len()),
            other => panic!("expected the placeholder titles bind, got {other:?}"),
        }
        match &binds[4] {
            Bind::Text(s) => assert_eq!(s, "sdk"),
            other => panic!("expected the template contact bind, got {other:?}"),
        }
    }

    #[test]
    fn places_only_gate_lets_every_world_through() {
        let (clause, _) = build_content_quality_condition(&feed(true, false), true, 1).unwrap();
        assert!(
            clause.starts_with("(world IS TRUE OR (highlighted IS TRUE OR ("),
            "{clause}"
        );
    }

    #[test]
    fn worlds_only_gate_lets_every_place_through_and_never_checks_roads() {
        let (clause, _) = build_content_quality_condition(&feed(false, true), true, 1).unwrap();
        assert!(
            clause.starts_with("(world IS FALSE OR (highlighted IS TRUE OR ("),
            "{clause}"
        );
        assert!(!clause.contains("road_positions"), "{clause}");
    }

    #[test]
    fn road_leg_is_left_out_until_the_table_exists() {
        let (clause, binds) = build_content_quality_condition(&feed(true, true), false, 1).unwrap();
        assert!(!clause.contains("road_positions"), "{clause}");
        assert_eq!(binds.len(), 5);
    }

    #[test]
    fn creator_leg_accepts_owner_world_deployer_world_name_or_a_named_contact() {
        let (clause, _) = build_content_quality_condition(&feed(true, true), false, 1).unwrap();
        assert!(
            clause.contains("TRIM(COALESCE(raw->>'owner', '')) <> ''"),
            "{clause}"
        );
        assert!(
            !clause.contains("COALESCE(creator_address"),
            "the places arm reads upstream's owner, not the deployer column: {clause}"
        );
        assert!(
            clause
                .contains("(world IS TRUE AND TRIM(COALESCE(raw->>'creator_address', '')) <> '')"),
            "{clause}"
        );
        assert!(
            clause.contains("(world IS TRUE AND TRIM(COALESCE(world_name, '')) <> '')"),
            "{clause}"
        );
        assert!(
            clause.contains("TRIM(COALESCE(raw->>'contact_name', '')) <> ''"),
            "{clause}"
        );
    }
}
