-- gig v2 schema. See docs/v2/SPEC.md section 3.

CREATE TABLE orders (
  id              INTEGER PRIMARY KEY,
  slug            TEXT NOT NULL UNIQUE,
  title           TEXT NOT NULL,
  material_path   TEXT,
  platform        TEXT,
  external_id     TEXT,
  project_type    TEXT NOT NULL,
  status          TEXT NOT NULL,
  currency        TEXT NOT NULL,
  price_minor     INTEGER,
  cut_ratio       REAL NOT NULL,
  dev_path        TEXT,
  archive_path    TEXT,
  client_words    TEXT,
  notes           TEXT NOT NULL DEFAULT '',
  created_at      TEXT NOT NULL,
  started_at      TEXT,
  delivered_at    TEXT,
  paid_at         TEXT,
  warranty_until  TEXT,
  archived_at     TEXT,
  cancelled_at    TEXT,
  cancel_reason   TEXT,
  legacy_id       INTEGER
);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_dev_path ON orders(dev_path);

CREATE TABLE drafts (
  id                INTEGER PRIMARY KEY,
  slug              TEXT NOT NULL UNIQUE,
  title             TEXT,
  material_path     TEXT,
  project_type      TEXT,
  notes_dir         TEXT NOT NULL,
  status            TEXT NOT NULL,
  drop_reason       TEXT,
  notes_snapshot    TEXT,
  promoted_order_id INTEGER REFERENCES orders(id) ON DELETE SET NULL,
  created_at        TEXT NOT NULL,
  closed_at         TEXT
);
CREATE INDEX idx_drafts_status ON drafts(status);

CREATE TABLE price_history (
  id          INTEGER PRIMARY KEY,
  order_id    INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  old_minor   INTEGER,
  new_minor   INTEGER,
  reason      TEXT,
  created_at  TEXT NOT NULL
);
CREATE INDEX idx_price_history_order ON price_history(order_id);

CREATE TABLE requirement_changes (
  id                INTEGER PRIMARY KEY,
  order_id          INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  description       TEXT NOT NULL,
  price_delta_minor INTEGER NOT NULL DEFAULT 0,
  created_at        TEXT NOT NULL
);
CREATE INDEX idx_requirement_changes_order ON requirement_changes(order_id);

CREATE TABLE packages (
  id             INTEGER PRIMARY KEY,
  order_id       INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  package_id     TEXT NOT NULL,
  kind           TEXT NOT NULL,
  dir            TEXT NOT NULL,
  manifest_path  TEXT NOT NULL,
  zip_path       TEXT NOT NULL,
  zip_sha256     TEXT,
  file_count     INTEGER,
  status         TEXT NOT NULL,
  checked_at     TEXT,
  sent_at        TEXT,
  channel        TEXT,
  uploader       TEXT,
  remote_url     TEXT,
  short_url      TEXT,
  expires_at     TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  UNIQUE(order_id, package_id)
);
CREATE INDEX idx_packages_order ON packages(order_id);

CREATE TABLE artifacts (
  id           INTEGER PRIMARY KEY,
  order_id     INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  local_path   TEXT,
  uploader     TEXT,
  remote_url   TEXT,
  short_url    TEXT,
  expires_at   TEXT,
  uploaded_at  TEXT NOT NULL
);
CREATE INDEX idx_artifacts_order ON artifacts(order_id);

CREATE TABLE events (
  id        INTEGER PRIMARY KEY,
  order_id  INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL,
  detail    TEXT,
  at        TEXT NOT NULL
);
CREATE INDEX idx_events_order ON events(order_id);

CREATE TABLE scorecards (
  order_id          INTEGER PRIMARY KEY REFERENCES orders(id) ON DELETE CASCADE,
  decisions         INTEGER,
  repeat_questions  INTEGER,
  days_to_preview   INTEGER,
  cleanups          INTEGER,
  check_rejections  INTEGER,
  report_reworks    INTEGER,
  score             INTEGER,
  note              TEXT,
  created_at        TEXT NOT NULL
);
