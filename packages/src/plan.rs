use proscenio_protocol::{Event, Stage};

use crate::check::open_private_alpm;
use crate::errors::format_error;
use crate::handle::watch;
use crate::output::emit;
use crate::transaction::plan_changes;

pub fn plan(db_path: &str) -> Result<(), String> {
    let (_lock_file, mut alpm) = open_private_alpm(db_path)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Synchronize));
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    emit(&Event::Stage(Stage::Prepare));
    emit(&Event::Planned(plan_changes(&mut alpm)?));
    Ok(())
}
