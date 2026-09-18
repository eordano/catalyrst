-- like_score ordering, one partial index per leg the list query cuts to a page
-- (PlaceOrderBy::LikeScore.column() must stay byte-identical to this expression).
CREATE INDEX IF NOT EXISTS place_like_score_guarded_place_only_idx
    ON place ((CASE WHEN jsonb_typeof(raw->'like_score') IN ('number', 'string') AND raw->>'like_score' ~ '^-?(([0-9]{1,255}|[0-9]{100}[0-9]{1,208})(\.([0-9]{1,255}|[0-9]{100}[0-9]{1,223}))?|[0-9]{1,3}(\.[0-9]{1,17})?[eE][+-]?([0-9]{1,2}|[12][0-9]{2}|30[0-5])|[0-9](\.[0-9]{1,17})?[eE][+-]?30[6-7]|1(\.(7[0-9]?|[0-6][0-9]{0,16}))?[eE][+-]?308)$' THEN (raw->>'like_score')::float8 END) DESC NULLS LAST, deployed_at DESC)
    WHERE disabled IS FALSE AND world IS FALSE;

CREATE INDEX IF NOT EXISTS place_like_score_guarded_world_only_idx
    ON place ((CASE WHEN jsonb_typeof(raw->'like_score') IN ('number', 'string') AND raw->>'like_score' ~ '^-?(([0-9]{1,255}|[0-9]{100}[0-9]{1,208})(\.([0-9]{1,255}|[0-9]{100}[0-9]{1,223}))?|[0-9]{1,3}(\.[0-9]{1,17})?[eE][+-]?([0-9]{1,2}|[12][0-9]{2}|30[0-5])|[0-9](\.[0-9]{1,17})?[eE][+-]?30[6-7]|1(\.(7[0-9]?|[0-6][0-9]{0,16}))?[eE][+-]?308)$' THEN (raw->>'like_score')::float8 END) DESC NULLS LAST, deployed_at DESC)
    WHERE disabled IS FALSE AND world IS TRUE;
