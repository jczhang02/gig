use crate::cli::{ConfigArgs, ConfigCommand};
use gig_core::config::{Config, Paths};
use gig_core::Result;
use rusqlite::Connection;

pub fn run(_conn: &Connection, args: ConfigArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    match args.command {
        ConfigCommand::Get(a) => cmd_get(&paths, &a.key),
        ConfigCommand::Set(a) => cmd_set(&paths, &a.key, &a.value),
        ConfigCommand::Edit => cmd_edit(&paths),
    }
}

fn cmd_get(paths: &Paths, key: &str) -> Result<()> {
    let cfg = Config::load_or_default(&paths.config_file)?;
    let value = get_key(&cfg, key)?;
    println!("{value}");
    Ok(())
}

fn cmd_set(paths: &Paths, key: &str, value: &str) -> Result<()> {
    let mut cfg = Config::load_or_default(&paths.config_file)?;
    set_key(&mut cfg, key, value)?;
    cfg.save(&paths.config_file)?;
    println!("set {key} = {value}");
    Ok(())
}

fn cmd_edit(paths: &Paths) -> Result<()> {
    // Create default config if missing.
    if !paths.config_file.exists() {
        let cfg = Config::default();
        cfg.save(&paths.config_file)?;
        println!("created default config at {}", paths.config_file.display());
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    std::process::Command::new(&editor)
        .arg(&paths.config_file)
        .status()
        .map_err(|e| {
            gig_core::Error::Invalid(format!("failed to launch editor {editor:?}: {e}"))
        })?;
    Ok(())
}

fn get_key(cfg: &Config, key: &str) -> Result<String> {
    Ok(match key {
        "general.dev_root" => cfg.general.dev_root.display().to_string(),
        "general.archive_root" => cfg.general.archive_root.display().to_string(),
        "general.default_cut_ratio" => cfg.general.default_cut_ratio.to_string(),
        "general.default_currency" => cfg.general.default_currency.clone(),
        "delivery.default_uploader" => cfg.delivery.default_uploader.clone(),
        "pack.default_format" => cfg.pack.default_format.clone(),
        other => {
            return Err(gig_core::Error::Invalid(format!(
                "unknown key {other:?}; supported: general.dev_root, general.archive_root, \
                 general.default_cut_ratio, general.default_currency, \
                 delivery.default_uploader, pack.default_format"
            )))
        }
    })
}

fn set_key(cfg: &mut Config, key: &str, value: &str) -> Result<()> {
    match key {
        "general.dev_root" => cfg.general.dev_root = value.into(),
        "general.archive_root" => cfg.general.archive_root = value.into(),
        "general.default_cut_ratio" => {
            cfg.general.default_cut_ratio = value
                .parse::<f64>()
                .map_err(|_| gig_core::Error::Invalid(format!("{value:?} is not a valid float")))?;
        }
        "general.default_currency" => cfg.general.default_currency = value.into(),
        "delivery.default_uploader" => cfg.delivery.default_uploader = value.into(),
        "pack.default_format" => cfg.pack.default_format = value.into(),
        other => return Err(gig_core::Error::Invalid(format!("unknown key {other:?}"))),
    }
    Ok(())
}
