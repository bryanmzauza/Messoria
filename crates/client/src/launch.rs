//! Starting another instance of the game. The title screen starts a world
//! or joins one by running the game again with the arguments for it and
//! leaving; the game comes back to the title screen the same way, with a
//! word on why if it was sent away.

use std::{ffi::OsString, io, process::Command};

use bevy::prelude::*;

/// The game's option naming the folder it keeps its data in. Every
/// instance started from this one keeps it, or the worlds, the profile and
/// the settings would move back to the player's own data folder halfway.
const DATA_DIR_OPTION: &str = "--data-dir";

/// How this instance was started: from the title screen, to be returned
/// to, or from a terminal.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct Launched {
    pub from_title: bool,
}

/// Runs the game again with `args`, detached from this instance, in the
/// same data folder.
///
/// # Errors
///
/// If the executable cannot be found or started.
pub(crate) fn start(args: &[String]) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .args(kept(std::env::args_os().skip(1)))
        .args(args)
        .spawn()
        .map(|_| ())
}

/// The arguments among this instance's `own` that the next one keeps: the
/// data folder, in either of the forms the option takes.
fn kept(own: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut kept = Vec::new();
    let mut own = own.into_iter();
    while let Some(arg) = own.next() {
        if arg == DATA_DIR_OPTION {
            kept.push(arg);
            kept.extend(own.next());
        } else if arg
            .to_str()
            .and_then(|arg| arg.strip_prefix(DATA_DIR_OPTION))
            .is_some_and(|rest| rest.starts_with('='))
        {
            kept.push(arg);
        }
    }
    kept
}

/// Starts the title screen, showing `notice` if there is one.
pub fn back_to_title(notice: Option<&str>) {
    let mut args = vec!["--from-title".to_owned()];
    if let Some(notice) = notice {
        args.push("--notice".to_owned());
        args.push(notice.to_owned());
    }
    if let Err(error) = start(&args) {
        error!("the title screen could not be started: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments<const N: usize>(args: [&str; N]) -> Vec<OsString> {
        args.into_iter().map(OsString::from).collect()
    }

    #[test]
    fn instances_started_keep_the_data_folder_and_nothing_else() {
        let own = arguments([
            "--from-title",
            "--data-dir",
            "D:/Games/Messoria data",
            "--world",
            "D:/Games/Messoria data/worlds/Farm",
            "--notice",
            "That world is full.",
        ]);
        assert_eq!(
            kept(own),
            arguments(["--data-dir", "D:/Games/Messoria data"])
        );
        assert_eq!(
            kept(arguments(["--data-dir=portable", "--connect", "10.0.0.2"])),
            arguments(["--data-dir=portable"])
        );
        assert!(kept(arguments(["--world", "farm", "--data-directory"])).is_empty());
        assert!(kept(arguments([])).is_empty());
    }
}
