CREATE TABLE IF NOT EXISTS packages (
  source TEXT NOT NULL, package_id TEXT NOT NULL, name TEXT NOT NULL, publisher TEXT NOT NULL,
  latest_version TEXT NOT NULL, description TEXT NOT NULL, icon_url TEXT, rating REAL, rating_count INTEGER,
  is_paid INTEGER NOT NULL DEFAULT 0, packaging TEXT NOT NULL, detail_json TEXT, fetched_at INTEGER NOT NULL,
  PRIMARY KEY (source, package_id));
CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name);
CREATE TABLE IF NOT EXISTS installs (
  package_id TEXT PRIMARY KEY NOT NULL, source TEXT NOT NULL, state TEXT NOT NULL,
  installed_version TEXT, target_version TEXT, pinned INTEGER NOT NULL DEFAULT 0, auto_update INTEGER NOT NULL DEFAULT 1,
  last_update_check INTEGER, last_updated INTEGER);
CREATE TABLE IF NOT EXISTS sources (
  kind TEXT PRIMARY KEY NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, priority INTEGER NOT NULL DEFAULT 0,
  display_name TEXT NOT NULL, extra_config TEXT, last_sync INTEGER);
CREATE TABLE IF NOT EXISTS operation_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, level TEXT NOT NULL, operation TEXT,
  package_id TEXT, message TEXT NOT NULL, detail TEXT);
CREATE INDEX IF NOT EXISTS idx_operation_log_package ON operation_log(package_id, ts);
