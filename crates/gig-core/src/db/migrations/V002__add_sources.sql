CREATE TABLE sources (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL UNIQUE,
    cut_ratio REAL NOT NULL,
    notes     TEXT
);

-- Orders now reference sources. Keep source_org TEXT for display/backward compat.
ALTER TABLE orders ADD COLUMN source_id INTEGER REFERENCES sources(id);
