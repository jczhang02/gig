//! User theme files and the catalogue of available themes
//! (docs/v2/TUI-DESIGN.md sections 14.2 and 15).
//!
//! A theme file is `<themes_dir>/<name>.toml` with exactly the 15 slot keys
//! of the built-ins, each `"#rrggbb"`. A user file shadows the built-in of
//! the same name. Broken files never block anything: they are reported and
//! skipped.

use crate::theme::{Theme, SLOTS};
use ratatui::style::Color;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// A theme file that could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    pub path: PathBuf,
    /// Theme name (file stem).
    pub name: String,
    /// What is wrong, naming the key when there is one.
    pub detail: String,
}

impl Broken {
    /// `theme <name>: <detail>`.
    pub fn reason(&self) -> String {
        format!("theme {}: {}", self.name, self.detail)
    }
}

/// Every available theme: the built-ins in their order (a user file of the
/// same name takes the built-in's place), then the other user themes
/// alphabetically.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub themes: Vec<Theme>,
    pub broken: Vec<Broken>,
}

impl Catalog {
    /// Built-ins plus the files in `dir`. A missing directory is not an error.
    pub fn load(dir: &Path) -> Self {
        let mut themes: Vec<Theme> = Theme::BUILTIN.to_vec();
        let mut broken = Vec::new();
        let mut user = Vec::new();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                let mut files: Vec<(String, PathBuf)> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file())
                    .filter_map(|p| theme_name(&p).map(|n| (n, p)))
                    .collect();
                files.sort();
                for (name, path) in files {
                    match read_and_parse(&path, &name) {
                        Ok(t) => user.push(t),
                        Err(detail) => broken.push(Broken { path, name, detail }),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => broken.push(Broken {
                path: dir.to_path_buf(),
                name: String::new(),
                detail: e.to_string(),
            }),
        }
        for t in user {
            match themes.iter_mut().find(|b| b.name == t.name) {
                Some(slot) => *slot = t,
                None => themes.push(t),
            }
        }
        Self { themes, broken }
    }

    /// Names in listing and cycling order.
    pub fn names(&self) -> Vec<&str> {
        self.themes.iter().map(|t| t.name.as_ref()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.iter().find(|t| t.name == name)
    }

    /// The theme to open with, plus a warning for the message row. `None`
    /// selects `gig-dark`. An unknown name or a broken file falls back to
    /// `gig-dark` with the reason; a theme below the contrast thresholds
    /// still loads, with its first failure as the warning.
    pub fn pick(&self, name: Option<&str>) -> (Theme, Option<String>) {
        let default = || {
            self.get(Theme::DEFAULT_NAME)
                .cloned()
                .unwrap_or(Theme::GIG_DARK)
        };
        let name = name.unwrap_or(Theme::DEFAULT_NAME);
        if let Some(b) = self.broken.iter().find(|b| b.name == name) {
            let warning = format!("{}, using {}", b.reason(), Theme::DEFAULT_NAME);
            return (default(), Some(warning));
        }
        match self.get(name) {
            Some(t) => {
                let warning = t
                    .contrast_failure()
                    .map(|f| format!("theme {}: {f}", t.name));
                (t.clone(), warning)
            }
            None => (
                default(),
                Some(format!(
                    "unknown theme \"{name}\", using {}",
                    Theme::DEFAULT_NAME
                )),
            ),
        }
    }
}

/// `Some(stem)` for `<stem>.toml` with a stem of `[a-z0-9][a-z0-9-]*`.
fn theme_name(path: &Path) -> Option<String> {
    if path.extension()? != "toml" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let mut chars = stem.chars();
    let first = chars.next()?;
    let ok = (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    ok.then(|| stem.to_string())
}

/// Parse one theme file. The name is the file stem. Errors read
/// `theme <name>: <reason>` and name the offending key.
pub fn load_file(path: &Path) -> Result<Theme, String> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    read_and_parse(path, &name).map_err(|detail| format!("theme {name}: {detail}"))
}

fn read_and_parse(path: &Path, name: &str) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(name, &text)
}

/// Parse theme file content into a theme called `name`.
pub fn parse(name: &str, text: &str) -> Result<Theme, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| {
        let msg = e.message().trim_end().to_string();
        format!("not valid TOML: {msg}")
    })?;
    if let Some(key) = table.keys().find(|k| !SLOTS.contains(&k.as_str())) {
        return Err(format!("unknown key \"{key}\""));
    }
    if let Some(key) = SLOTS.iter().find(|k| !table.contains_key(**k)) {
        return Err(format!("missing key \"{key}\""));
    }
    let mut theme = Theme {
        name: Cow::Owned(name.to_string()),
        ..Theme::GIG_DARK
    };
    for key in SLOTS {
        let colour = table[key]
            .as_str()
            .and_then(parse_hex)
            .ok_or_else(|| format!("\"{key}\" is not #rrggbb"))?;
        *theme.slot_mut(key).expect("slot") = colour;
    }
    Ok(theme)
}

/// `#rrggbb`, hex digits in either case.
fn parse_hex(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The complete example of TUI-DESIGN.md section 15.
    const MOCHA_SOFT: &str = r##"
bg       = "#1e1e2e"
surface  = "#181825"
sel      = "#313244"
border   = "#45475a"
text     = "#cdd6f4"
muted    = "#a6adc8"
dim      = "#7f849c"
accent   = "#b4befe"
key      = "#cdd6f4"
unpaid   = "#f38ba8"
warranty = "#fab387"
queued   = "#89b4fa"
archived = "#969cb4"
bar      = "#6c7086"
bar_now  = "#B4BEFE"
"##;

    fn write(dir: &Path, file: &str, text: &str) -> PathBuf {
        let path = dir.join(file);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_custom_theme_file_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "mocha-soft.toml", MOCHA_SOFT);
        let t = load_file(&path).unwrap();
        assert_eq!(t.name, "mocha-soft");
        assert_eq!(t.accent, Color::Rgb(0xb4, 0xbe, 0xfe));
        assert_eq!(t.bar_now, t.accent, "upper-case hex digits parse");
        assert_eq!(t.bg, Theme::CATPPUCCIN_MOCHA.bg);
        assert!(t.contrast_failure().is_none());

        let cat = Catalog::load(dir.path());
        assert!(cat.broken.is_empty());
        assert_eq!(cat.names().len(), 9);
        assert_eq!(cat.names()[8], "mocha-soft");
        let (picked, warning) = cat.pick(Some("mocha-soft"));
        assert_eq!(picked, t);
        assert_eq!(warning, None);
    }

    #[test]
    fn bad_files_name_the_key() {
        let missing = MOCHA_SOFT.replace("bar      = \"#6c7086\"\n", "");
        assert_eq!(
            parse("mocha-soft", &missing).unwrap_err(),
            "missing key \"bar\""
        );
        let unknown = format!("{MOCHA_SOFT}error = \"#ff0000\"\n");
        assert_eq!(parse("x", &unknown).unwrap_err(), "unknown key \"error\"");
        for bad in ["\"#a6adc\"", "\"a6adc8\"", "\"#a6adcg\"", "12"] {
            let text = MOCHA_SOFT.replace("\"#a6adc8\"", bad);
            assert_eq!(
                parse("x", &text).unwrap_err(),
                "\"muted\" is not #rrggbb",
                "{bad}"
            );
        }
        assert!(parse("x", "bg = ")
            .unwrap_err()
            .starts_with("not valid TOML"));

        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "mocha-soft.toml", &missing);
        assert_eq!(
            load_file(&path).unwrap_err(),
            "theme mocha-soft: missing key \"bar\""
        );
    }

    #[test]
    fn user_files_shadow_builtins_and_bad_ones_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nord.toml", MOCHA_SOFT);
        write(dir.path(), "zz-broken.toml", "bg = \"#000000\"\n");
        write(dir.path(), "aa-extra.toml", MOCHA_SOFT);
        write(dir.path(), "Upper.toml", MOCHA_SOFT);
        write(dir.path(), "notes.txt", "ignored");
        let cat = Catalog::load(dir.path());
        assert_eq!(
            cat.names(),
            [
                "gig-dark",
                "gig-light",
                "catppuccin-mocha",
                "catppuccin-latte",
                "tokyonight",
                "gruvbox-dark",
                "nord",
                "dracula",
                "aa-extra"
            ]
        );
        assert_eq!(
            cat.get("nord").unwrap().accent,
            Color::Rgb(0xb4, 0xbe, 0xfe)
        );
        assert_eq!(cat.broken.len(), 1);
        assert_eq!(cat.broken[0].name, "zz-broken");

        let (t, w) = cat.pick(Some("zz-broken"));
        assert_eq!(t, Theme::GIG_DARK);
        assert_eq!(
            w.unwrap(),
            "theme zz-broken: missing key \"surface\", using gig-dark"
        );
        let (t, w) = cat.pick(Some("nrod"));
        assert_eq!(t, Theme::GIG_DARK);
        assert_eq!(w.unwrap(), "unknown theme \"nrod\", using gig-dark");
        assert_eq!(cat.pick(None), (Theme::GIG_DARK, None));
    }

    #[test]
    fn a_low_contrast_file_loads_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "murky.toml",
            &MOCHA_SOFT.replace("\"#a6adc8\"", "\"#45475a\""),
        );
        let (t, w) = Catalog::load(dir.path()).pick(Some("murky"));
        assert_eq!(t.name, "murky");
        assert!(w.unwrap().starts_with("theme murky: muted "), "warning");
    }

    #[test]
    fn missing_directory_gives_the_builtins() {
        let cat = Catalog::load(Path::new("/nonexistent/gig/themes"));
        assert_eq!(cat.names().len(), 8);
        assert!(cat.broken.is_empty());
    }
}
