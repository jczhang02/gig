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
