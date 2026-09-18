ALTER TABLE collection_publications DROP CONSTRAINT collection_publications_status_check;
ALTER TABLE collection_publications ADD CONSTRAINT collection_publications_status_check
    CHECK (status IN ('prepared', 'signing', 'submitted', 'published', 'reverted'));
