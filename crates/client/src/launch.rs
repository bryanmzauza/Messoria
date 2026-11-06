//! Starting another instance of the game. The title screen starts a world
//! or joins one by running the game again with the arguments for it and
//! leaving; the game comes back to the title screen the same way, with a
//! word on why if it was sent away.

use std::{io, process::Command};

use bevy::prelude::*;

/// How this instance was started: from the title screen, to be returned
/// to, or from a terminal.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct Launched {
    pub from_title: bool,
}

/// Runs the game again with `args`, detached from this instance.
///
/// # Errors
///
/// If the executable cannot be found or started.
pub(crate) fn start(args: &[String]) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe).args(args).spawn().map(|_| ())
}

/// Starts the title screen, showing `notice` if there is one.
pub(crate) fn back_to_title(notice: Option<&str>) {
    let mut args = vec!["--from-title".to_owned()];
    if let Some(notice) = notice {
        args.push("--notice".to_owned());
        args.push(notice.to_owned());
    }
    if let Err(error) = start(&args) {
        error!("the title screen could not be started: {error}");
    }
}
