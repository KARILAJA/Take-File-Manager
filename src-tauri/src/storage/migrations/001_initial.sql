-- Initial TurboGet Database Schema
CREATE TABLE IF NOT EXISTS downloads (
  id            TEXT PRIMARY KEY,
  url           TEXT NOT NULL,
  final_url     TEXT,
  file_name     TEXT NOT NULL,
  save_dir      TEXT NOT NULL,
  total_bytes   INTEGER,
  downloaded    INTEGER NOT NULL DEFAULT 0,
  status        TEXT NOT NULL,
  error         TEXT,
  resumable     INTEGER NOT NULL DEFAULT 1,
  etag          TEXT,
  last_modified TEXT,
  category_id   TEXT,
  queue_id      TEXT,
  priority      INTEGER NOT NULL DEFAULT 0,
  segments_cfg  INTEGER NOT NULL DEFAULT 8,
  speed_limit   INTEGER,
  headers_json  TEXT,
  checksum_kind TEXT,
  checksum_value TEXT,
  created_at    INTEGER NOT NULL,
  started_at    INTEGER,
  completed_at  INTEGER
);

CREATE TABLE IF NOT EXISTS segments (
  download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
  idx         INTEGER NOT NULL,
  start_byte  INTEGER NOT NULL,
  end_byte    INTEGER NOT NULL,
  written     INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (download_id, idx)
);

CREATE TABLE IF NOT EXISTS queues (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  max_concurrent INTEGER NOT NULL DEFAULT 3,
  speed_limit INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS schedules (
  id TEXT PRIMARY KEY,
  queue_id TEXT REFERENCES queues(id),
  days_mask INTEGER NOT NULL,
  start_time TEXT,
  stop_time TEXT,
  after_action TEXT,
  after_command TEXT,
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS categories (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  extensions TEXT NOT NULL,
  save_dir TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL,
  level TEXT NOT NULL,
  download_id TEXT,
  message TEXT NOT NULL
);
