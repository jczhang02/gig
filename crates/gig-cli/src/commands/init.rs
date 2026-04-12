use crate::cli::InitArgs;
use crate::ui;
use gig_core::config::{Config, Paths};
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::services::init::init_project;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection, args: InitArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;

    let order = find_by_id_or_slug(conn, &args.id)?;
    let dev_root = config.general.dev_root;
    let order = init_project(conn, order.id, &dev_root, args.slug.as_deref())?;

    println!("initialised project for order #{}", order.id);
    println!("  dev_path : {}", order.dev_path.as_deref().unwrap_or("—"));
    println!("{}", ui::order_detail(&order));
    Ok(())
}
