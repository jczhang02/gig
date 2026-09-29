-- gig v0.1 initial schema
PRAGMA foreign_keys = ON;

CREATE TABLE clients (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    display_name    TEXT NOT NULL,
    wechat_contact  TEXT,
    source_org      TEXT,
    notes           TEXT,
    first_seen_at   INTEGER NOT NULL
);

CREATE INDEX idx_clients_source_org ON clients(source_org);

CREATE TABLE orders (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    slug            TEXT UNIQUE,
    external_id     TEXT,
    title           TEXT NOT NULL,
    client_id       INTEGER REFERENCES clients(id) ON DELETE SET NULL,
    source_org      TEXT,
    status          TEXT NOT NULL,
    quoted_price    INTEGER,    -- in minor currency units (e.g. cents)
    final_price     INTEGER,
    my_cut_ratio    REAL NOT NULL,
    currency        TEXT NOT NULL,
    dev_path        TEXT,
    archive_path    TEXT,
    notes           TEXT,
    created_at      INTEGER NOT NULL,
    accepted_at     INTEGER,
    delivered_at    INTEGER,
    paid_at         INTEGER,
    archived_at     INTEGER
);

CREATE INDEX idx_orders_status      ON orders(status);
CREATE INDEX idx_orders_client_id   ON orders(client_id);
CREATE INDEX idx_orders_dev_path    ON orders(dev_path);
CREATE INDEX idx_orders_archive_path ON orders(archive_path);

CREATE TABLE price_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id    INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    old_price   INTEGER,
    new_price   INTEGER,
    reason      TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX idx_price_history_order_id ON price_history(order_id);

CREATE TABLE requirement_changes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id    INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    price_delta INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE INDEX idx_req_changes_order_id ON requirement_changes(order_id);

CREATE TABLE delivery_artifacts (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id       INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    local_path     TEXT,
    uploader_name  TEXT,
    remote_url     TEXT,
    expires_at     INTEGER,
    uploaded_at    INTEGER NOT NULL
);

CREATE INDEX idx_delivery_order_id ON delivery_artifacts(order_id);

CREATE TABLE tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE
);

CREATE TABLE order_tags (
    order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (order_id, tag_id)
);

CREATE TABLE sources (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL UNIQUE,
    cut_ratio REAL NOT NULL,
    notes     TEXT
);

-- Orders now reference sources. Keep source_org TEXT for display/backward compat.
ALTER TABLE orders ADD COLUMN source_id INTEGER REFERENCES sources(id);

ALTER TABLE orders ADD COLUMN project_type TEXT;

CREATE TABLE quote_drafts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    slug                TEXT NOT NULL UNIQUE,
    title               TEXT NOT NULL,
    client_label        TEXT,
    source_org          TEXT,
    project_type        TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'quote_draft',
    summary             TEXT NOT NULL,
    quote_min           INTEGER,
    quote_recommended   INTEGER,
    quote_max           INTEGER,
    currency            TEXT NOT NULL DEFAULT 'CNY',
    xdg_path            TEXT NOT NULL,
    drop_reason         TEXT,
    promoted_order_id   INTEGER REFERENCES orders(id),
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    quoted_at           TEXT,
    sent_at             TEXT,
    accepted_at         TEXT,
    dropped_at          TEXT
);

CREATE INDEX idx_quote_drafts_status ON quote_drafts(status);
CREATE INDEX idx_quote_drafts_project_type ON quote_drafts(project_type);
CREATE INDEX idx_quote_drafts_promoted_order_id ON quote_drafts(promoted_order_id);

CREATE TABLE order_workflow (
    order_id                    INTEGER PRIMARY KEY REFERENCES orders(id) ON DELETE CASCADE,
    project_type                TEXT,
    gig_dir                     TEXT,
    index_path                  TEXT,
    job_path                    TEXT,
    quote_path                  TEXT,
    plan_md_path                TEXT,
    plan_html_path              TEXT,
    plan_ready_at               TEXT,
    plan_approved_at            TEXT,
    plan_rejected_at            TEXT,
    plan_rejection_reason       TEXT,
    acceptance_path             TEXT,
    acceptance_completed_at     TEXT,
    latest_delivery_dir         TEXT,
    latest_client_package_path  TEXT,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);

CREATE INDEX idx_order_workflow_project_type ON order_workflow(project_type);

CREATE TABLE delivery_packages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id        INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    delivery_date   TEXT NOT NULL,
    delivery_dir    TEXT NOT NULL,
    client_dir      TEXT NOT NULL,
    manifest_path   TEXT NOT NULL,
    package_path    TEXT,
    status          TEXT NOT NULL DEFAULT 'prepared',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX idx_delivery_packages_order_id ON delivery_packages(order_id);
CREATE INDEX idx_delivery_packages_status ON delivery_packages(status);

ALTER TABLE order_workflow ADD COLUMN work_started_at TEXT;
