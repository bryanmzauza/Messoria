//! Who may play here: every client says hello with its protocol version
//! and is admitted or told why not, the host opens and closes the world to
//! guests, and everyone is told who is here.
//!
//! A client is only given a character once admitted. A rejected client is
//! told the reason and leaves on its own; nothing is replicated to it
//! beyond the world's few global things. The host's own client is always
//! admitted, and only it may open or close the world.

use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
use messoria_shared::{
    network::PROTOCOL_VERSION,
    protocol::{
        FeedbackChannel, Hello, Hosting, PlayerId, PlayerName, Rejected, Rejection, SetHospitality,
        WorldClock,
    },
};

pub(crate) struct HostingPlugin {
    pub bind_addr: SocketAddr,
    pub open_to_guests: bool,
    pub max_players: u32,
}

impl Plugin for HostingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Hospitality {
            open: self.open_to_guests,
            max_players: self.max_players,
            // A world listening on loopback only cannot be opened to anyone.
            port: (!self.bind_addr.ip().is_loopback()).then_some(self.bind_addr.port()),
        })
        .add_systems(
            PreUpdate,
            (admit, set_hospitality).after(MessageSystems::Receive),
        )
        .add_systems(Update, publish);
    }
}

/// Whether guests may join, how many players at most, and the port guests
/// come in on, if the world listens for any.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct Hospitality {
    pub open: bool,
    pub max_players: u32,
    pub port: Option<u16>,
}

/// A connection that said hello and was let in.
#[derive(Component)]
pub(crate) struct Admitted;

/// Whether a hello is welcome, given the world's hospitality and how many
/// players are in it already.
fn decide(
    hello: Hello,
    local: bool,
    hospitality: Hospitality,
    players: u32,
) -> Result<(), Rejection> {
    if hello.protocol != PROTOCOL_VERSION {
        return Err(Rejection::OtherVersion);
    }
    if local {
        return Ok(());
    }
    if !hospitality.open {
        return Err(Rejection::Closed);
    }
    if players >= hospitality.max_players {
        return Err(Rejection::Full);
    }
    Ok(())
}

fn admit(
    hospitality: Res<Hospitality>,
    admitted: Query<(), With<Admitted>>,
    mut clients: Query<
        (
            Entity,
            &RemoteId,
            &mut MessageReceiver<Hello>,
            &mut MessageSender<Rejected>,
        ),
        (With<ClientOf>, Without<Admitted>),
    >,
    mut commands: Commands,
) {
    let mut players = u32::try_from(admitted.iter().count()).unwrap_or(u32::MAX);
    for (connection, peer, mut hellos, mut answers) in &mut clients {
        let Some(hello) = hellos.receive().last() else {
            continue;
        };
        let local = matches!(peer.0, PeerId::Local(_));
        match decide(hello, local, *hospitality, players) {
            Ok(()) => {
                players = players.saturating_add(1);
                commands.entity(connection).insert(Admitted);
            }
            Err(reason) => {
                info!("turned {:?} away: {reason}", peer.0);
                answers.send::<FeedbackChannel>(Rejected { reason });
            }
        }
    }
}

/// The host opens or closes the world; closing it sends every guest away.
fn set_hospitality(
    mut hospitality: ResMut<Hospitality>,
    mut clients: Query<(&RemoteId, &mut MessageReceiver<SetHospitality>), With<ClientOf>>,
    mut guests: Query<
        (Entity, &RemoteId, &mut MessageSender<Rejected>),
        (With<ClientOf>, With<Admitted>),
    >,
    mut commands: Commands,
) {
    let mut wanted = None;
    for (peer, mut requests) in &mut clients {
        for SetHospitality { open } in requests.receive() {
            if matches!(peer.0, PeerId::Local(_)) {
                wanted = Some(open);
            } else {
                debug!("ignored a guest asking to open or close the world");
            }
        }
    }
    let Some(open) = wanted.filter(|&open| open != hospitality.open) else {
        return;
    };
    if open && hospitality.port.is_none() {
        debug!("the world listens on loopback only and cannot be opened");
        return;
    }
    hospitality.open = open;
    info!("the world is now {}", if open { "open" } else { "closed" });
    if !open {
        for (connection, peer, mut answers) in &mut guests {
            if !matches!(peer.0, PeerId::Local(_)) {
                answers.send::<FeedbackChannel>(Rejected {
                    reason: Rejection::Closed,
                });
                commands.entity(connection).remove::<Admitted>();
            }
        }
    }
}

/// Keeps the clock entity saying whether the world is open, on which
/// port, and who is here, for every client's menu.
fn publish(
    hospitality: Res<Hospitality>,
    names: Query<&PlayerName, With<PlayerId>>,
    clock: Single<(Entity, Option<&mut Hosting>), With<WorldClock>>,
    mut commands: Commands,
) {
    let (entity, hosting) = clock.into_inner();
    let mut here: Vec<String> = names.iter().map(|name| name.0.clone()).collect();
    here.sort_unstable();
    let now = Hosting {
        open: hospitality.open,
        port: hospitality.port,
        here,
    };
    match hosting {
        Some(mut hosting) => {
            hosting.set_if_neq(now);
        }
        None => {
            commands.entity(entity).insert(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: Hospitality = Hospitality {
        open: true,
        max_players: 2,
        port: Some(5717),
    };

    #[test]
    fn guests_are_admitted_while_the_world_is_open_and_has_room() {
        let hello = Hello {
            protocol: PROTOCOL_VERSION,
        };
        assert_eq!(decide(hello, false, OPEN, 1), Ok(()));
        assert_eq!(decide(hello, false, OPEN, 2), Err(Rejection::Full));
        let closed = Hospitality {
            open: false,
            ..OPEN
        };
        assert_eq!(decide(hello, false, closed, 0), Err(Rejection::Closed));
        assert_eq!(decide(hello, true, closed, 9), Ok(()), "the host always is");
    }

    #[test]
    fn another_version_is_turned_away_even_at_home() {
        let hello = Hello {
            protocol: PROTOCOL_VERSION + 1,
        };
        assert_eq!(decide(hello, true, OPEN, 0), Err(Rejection::OtherVersion));
    }
}
