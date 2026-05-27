use crate::cli::{TemplateArgs, TemplateCommand};
use gig_core::config::Paths;
use gig_core::templates::EMBEDDED_TEMPLATES;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(_conn: &Connection, args: TemplateArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    match args.command {
        TemplateCommand::Ls => cmd_ls(&paths),
        TemplateCommand::Show(a) => cmd_show(&paths, &a.name),
        TemplateCommand::Edit(a) => cmd_edit(&paths, &a.name),
    }
}

fn cmd_ls(paths: &Paths) -> Result<()> {
    println!("embedded templates:");
    for (name, _) in EMBEDDED_TEMPLATES {
        println!("  {name}  (built-in)");
    }

    let user_dir = &paths.templates_dir;
    if user_dir.is_dir() {
        let mut found_user = false;
        if let Ok(entries) = std::fs::read_dir(user_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname = fname.to_string_lossy();
                if fname.ends_with(".j2") || fname.ends_with(".md") {
                    if !found_user {
                        println!("\nuser templates ({}):", user_dir.display());
                        found_user = true;
                    }
                    println!("  {fname}");
                }
            }
        }
    }
    Ok(())
}

fn cmd_show(paths: &Paths, name: &str) -> Result<()> {
    // Check user dir first.
    let user_path = paths.templates_dir.join(format!("{name}.j2"));
    if user_path.exists() {
        let content = std::fs::read_to_string(&user_path)
            .map_err(|e| gig_core::Error::PathUnavailable(user_path.clone(), e))?;
        print!("{content}");
        return Ok(());
    }

    // Fall back to embedded.
    let content = find_embedded(name)
        .ok_or_else(|| gig_core::Error::Invalid(format!("template {name:?} not found")))?;
    print!("{content}");
    Ok(())
}

fn cmd_edit(paths: &Paths, name: &str) -> Result<()> {
    let user_path = paths.templates_dir.join(format!("{name}.j2"));

    // If not in user dir yet, copy embedded default there first.
    if !user_path.exists() {
        let content = find_embedded(name)
            .ok_or_else(|| gig_core::Error::Invalid(format!("template {name:?} not found")))?;
        std::fs::create_dir_all(&paths.templates_dir)
            .map_err(|e| gig_core::Error::PathUnavailable(paths.templates_dir.clone(), e))?;
        std::fs::write(&user_path, content)
            .map_err(|e| gig_core::Error::PathUnavailable(user_path.clone(), e))?;
        println!("copied built-in template to {}", user_path.display());
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    std::process::Command::new(&editor)
        .arg(&user_path)
        .status()
        .map_err(|e| {
            gig_core::Error::Invalid(format!("failed to launch editor {editor:?}: {e}"))
        })?;
    Ok(())
}

fn find_embedded(name: &str) -> Option<&'static str> {
    for (k, v) in EMBEDDED_TEMPLATES {
        if k == name {
            return Some(v);
        }
    }
    None
}
