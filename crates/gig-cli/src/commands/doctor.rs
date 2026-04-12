use gig_core::context::doctor_path_checks;
use gig_core::Result;
use rusqlite::Connection;

pub fn run(conn: &Connection) -> Result<()> {
    let diagnostics = doctor_path_checks(conn)?;
    if diagnostics.is_empty() {
        println!("all paths OK");
    } else {
        for d in &diagnostics {
            println!("{d}");
        }
        println!("\n{} issue(s) found", diagnostics.len());
    }
    Ok(())
}
