use proscenio_protocol::{Event, Stage};
use std::path::Path;

use crate::check::open_private_alpm;
use crate::errors::format_error;
use crate::handle::watch;
use crate::output::emit;
use crate::private_database::{check_location, check_sync_location};
use crate::transaction::plan_changes;

pub fn plan(db_path: &str) -> Result<(), String> {
    check_location(Path::new(db_path))?;
    let (_lock_file, mut alpm) = open_private_alpm(db_path)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Synchronize));
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    check_sync_location(Path::new(db_path))?;
    emit(&Event::Stage(Stage::Prepare));
    emit(&Event::Planned(plan_changes(&mut alpm)?));
    Ok(())
}
