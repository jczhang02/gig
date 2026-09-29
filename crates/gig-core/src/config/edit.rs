//! In-place edits of config.toml through `toml_edit`: only the changed keys
//! are touched, so comments, key order and formatting survive. Writes are
//! atomic (temp file in the same directory, then rename).

use crate::{Error, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

/// The file as a document; an absent file is an empty one.
pub(crate) fn read_doc(path: &Path) -> Result<(String, DocumentMut)> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::PathUnavailable(path.to_path_buf(), e)),
    };
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    Ok((text, doc))
}

fn to_edit_value(v: &toml::Value) -> Result<toml_edit::Value> {
    // toml renders any non-table value (arrays and inline tables included) as
    // one TOML value, which toml_edit parses back with default formatting.
    v.to_string()
        .parse::<toml_edit::Value>()
        .map_err(|e| Error::Config(format!("cannot write value {v}: {e}")))
}

fn new_child(parent_inline: bool) -> Item {
    if parent_inline {
        Item::Value(toml_edit::Value::InlineTable(Default::default()))
    } else {
        let mut t = Table::new();
        // No header of its own while it holds only sub-tables.
        t.set_implicit(true);
        Item::Table(t)
    }
}

/// The table-like child `part` of `item`, created when missing.
fn child<'a>(item: &'a mut Item, part: &str, key: &str) -> Result<&'a mut Item> {
    let inline = item.is_inline_table();
    let t = item
        .as_table_like_mut()
        .ok_or_else(|| Error::InvalidInput(format!("{key} is not a table path")))?;
    if !t.contains_key(part) {
        t.insert(part, new_child(inline));
    }
    let c = t.get_mut(part).expect("just inserted");
    if c.as_table_like().is_none() {
        return Err(Error::InvalidInput(format!("{key} is not a table path")));
    }
    Ok(c)
}

/// Set `value` at `parent[name]`, keeping the old value's decoration (spacing
/// and trailing comment). Returns false when the value is already equal.
fn set_leaf(parent: &mut Item, name: &str, value: &toml::Value, key: &str) -> Result<bool> {
    let t = parent
        .as_table_like_mut()
        .ok_or_else(|| Error::InvalidInput(format!("{key} is not a table path")))?;
    let mut new = to_edit_value(value)?;
    match t.get_mut(name) {
        Some(Item::Value(old)) => {
            if same_value(old, value) {
                return Ok(false);
            }
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        Some(Item::Table(_)) | Some(Item::ArrayOfTables(_)) => {
            return Err(Error::InvalidInput(format!(
                "{key} is a table, not a value"
            )));
        }
        Some(Item::None) | None => {
            t.insert(name, Item::Value(new));
        }
    }
    Ok(true)
}

fn same_value(old: &toml_edit::Value, want: &toml::Value) -> bool {
    let text = format!("v = {}", old.clone().decorated("", ""));
    toml::from_str::<toml::Table>(&text)
        .ok()
        .and_then(|mut t| t.remove("v"))
        .is_some_and(|v| &v == want)
}

/// Set one dotted key, creating tables along the way.
pub(crate) fn set_key(doc: &mut DocumentMut, key: &str, value: &toml::Value) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidInput(format!("invalid config key {key:?}")));
    }
    let (last, dirs) = parts.split_last().expect("split never yields nothing");
    let mut cur = doc.as_item_mut();
    for part in dirs {
        cur = child(cur, part, key)?;
    }
    if let (toml::Value::Table(sub), true) = (value, cur.as_table_like().is_some()) {
        let c = child(cur, last, key)?;
        return merge(c, sub, key);
    }
    set_leaf(cur, last, value, key).map(|_| ())
}

/// Make `item` hold exactly `want`, touching only what differs.
pub(crate) fn merge(item: &mut Item, want: &toml::Table, key: &str) -> Result<()> {
    let stale: Vec<String> = item
        .as_table_like()
        .ok_or_else(|| Error::InvalidInput(format!("{key} is not a table path")))?
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| !want.contains_key(k))
        .collect();
    if let Some(t) = item.as_table_like_mut() {
        for k in stale {
            t.remove(&k);
        }
    }
    for (k, v) in want {
        let path = if key.is_empty() {
            k.clone()
        } else {
            format!("{key}.{k}")
        };
        match v {
            toml::Value::Table(sub) => {
                let replace = item
                    .as_table_like()
                    .and_then(|t| t.get(k))
                    .is_some_and(|c| c.as_table_like().is_none());
                if replace {
                    if let Some(t) = item.as_table_like_mut() {
                        t.remove(k);
                    }
                }
                let c = child(item, k, &path)?;
                merge(c, sub, &path)?;
            }
            _ => {
                let is_table = item
                    .as_table_like()
                    .and_then(|t| t.get(k))
                    .is_some_and(|c| c.as_table_like().is_some() || c.is_array_of_tables());
                if is_table {
                    if let Some(t) = item.as_table_like_mut() {
                        t.remove(k);
                    }
                }
                set_leaf(item, k, v, &path)?;
            }
        }
    }
    Ok(())
}

/// Replace `path` with `text` atomically. A symlinked config is followed so
/// the link itself survives; an existing file keeps its permissions.
pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
        super::secure_dir(&dir)?;
    }
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let io = |e| Error::PathUnavailable(tmp.clone(), e);
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io)?;
        f.write_all(text.as_bytes()).map_err(io)?;
        if let Ok(meta) = std::fs::metadata(&target) {
            f.set_permissions(meta.permissions()).map_err(io)?;
        }
        f.sync_all().map_err(io)?;
        std::fs::rename(&tmp, &target).map_err(|e| Error::PathUnavailable(target.clone(), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(text: &str, key: &str, v: toml::Value) -> String {
        let mut doc: DocumentMut = text.parse().unwrap();
        set_key(&mut doc, key, &v).unwrap();
        doc.to_string()
    }

    #[test]
    fn keeps_comments_order_and_decor() {
        let text = "\
# my gig config
[tui]
refresh_seconds = 2   # fast
icons = true

# general bits
[general]
warranty_days = 15 # days
default_currency = \"CNY\"
";
        let out = set(text, "general.warranty_days", toml::Value::Integer(30));
        assert_eq!(
            out,
            text.replace("warranty_days = 15", "warranty_days = 30")
        );
        // An equal value leaves the text byte for byte.
        let same = set(text, "tui.icons", toml::Value::Boolean(true));
        assert_eq!(same, text);
    }

    #[test]
    fn creates_missing_tables_and_keys() {
        let out = set("", "tui.mouse", toml::Value::Boolean(false));
        assert_eq!(out, "[tui]\nmouse = false\n");
        let out = set(
            "[general]\nwarranty_days = 3\n",
            "delivery.s3.hk.bucket",
            toml::Value::String("gig".into()),
        );
        assert_eq!(
            out,
            "[general]\nwarranty_days = 3\n\n[delivery.s3.hk]\nbucket = \"gig\"\n"
        );
        // Inline and dotted tables are edited where they are.
        let out = set(
            "tui = { icons = true }\n",
            "tui.mouse",
            toml::Value::Boolean(true),
        );
        assert_eq!(out, "tui = { icons = true , mouse = true }\n");
    }

    #[test]
    fn refuses_to_descend_into_values() {
        let mut doc: DocumentMut = "[general]\nwarranty_days = 3\n".parse().unwrap();
        let err = set_key(
            &mut doc,
            "general.warranty_days.x",
            &toml::Value::Integer(1),
        )
        .unwrap_err();
        assert_eq!(err.code(), "invalid_input");
        assert!(set_key(&mut doc, "general..x", &toml::Value::Integer(1)).is_err());
    }
}
