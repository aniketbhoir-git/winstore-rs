-- Detail freshness is tracked apart from summary freshness: a search refresh must not
-- make an old cached detail look new.
ALTER TABLE packages ADD COLUMN detail_fetched_at INTEGER;
UPDATE packages SET detail_fetched_at = fetched_at WHERE detail_json IS NOT NULL;
