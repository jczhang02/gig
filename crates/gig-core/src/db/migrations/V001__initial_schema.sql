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
