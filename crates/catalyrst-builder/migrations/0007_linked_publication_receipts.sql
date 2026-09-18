ALTER TABLE linked_collection_publications ADD COLUMN forum_url TEXT;
ALTER TABLE linked_collection_publications DROP CONSTRAINT linked_collection_publications_status_check;
UPDATE linked_collection_publications SET status='submitted' WHERE status='under_review';
ALTER TABLE linked_collection_publications ADD CONSTRAINT linked_collection_publications_status_check
    CHECK (status IN ('prepared', 'signing', 'authorized', 'submitted'));
