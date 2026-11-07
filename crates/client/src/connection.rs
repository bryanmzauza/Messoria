//! Joining a remote server or the world hosted by this same process, and
//! leaving it: turned away by the server, or cut off.
//!
//! A server that will not have this client says why; the client then
//! disconnects itself and, if it came from the title screen, goes back
//! there with the reason on show. Losing the connection goes the same way.

use std::{net::SocketAddr, time::Duration};

use bevy::prelude::*;
use lightyear::prelude::{server::Server, *};
use messoria_shared::{network, protocol::Rejected};

use crate::launch::{self, Launched};

/// Where the client's world lives.
#[derive(Resource, Clone, Debug)]
pub enum Session {
    /// The world is hosted in this process by `ServerPlugin`.
    Host,
    /// The world lives on another machine.
    Join {
        server_addr: SocketAddr,
        /// Who the player is, the same every time they join.
        player_id: u64,
        /// Extra delay applied to incoming packets, to exercise latency locally.
        simulated_latency: Option<Duration>,
    },
}

pub(crate) struct ConnectionPlugin {
    pub session: Session,
}

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.session.clone())
            .init_resource::<Farewell>();
        match self.session.clone() {
            // The in-process server entity is spawned during `Startup`.
            Session::Host => app.add_systems(PostStartup, join_hosted_world),
            Session::Join {
                server_addr,
                player_id,
                simulated_latency,
            } => app.add_systems(Startup, move |commands: Commands| -> Result {
                join_remote_world(commands, server_addr, player_id, simulated_latency)
            }),
        };

        app.add_systems(PreUpdate, hear_rejection.after(MessageSystems::Receive))
            .add_observer(log_connection)
            .add_observer(leave_on_failure)
            .add_observer(leave_on_disconnection);
    }
}

/// Why the server sent this client away, if it did.
#[derive(Resource, Default)]
struct Farewell(Option<String>);

fn join_hosted_world(server: Single<Entity, With<Server>>, mut commands: Commands) {
    let client = commands.spawn(network::host_client(*server)).id();
    commands.trigger(Connect { entity: client });
}

fn join_remote_world(
    mut commands: Commands,
    server_addr: SocketAddr,
    player_id: u64,
    simulated_latency: Option<Duration>,
) -> Result {
    let client = commands
        .spawn(network::remote_client(
            server_addr,
            player_id,
            simulated_latency,
        )?)
        .id();
    commands.trigger(Connect { entity: client });
    info!("connecting to {server_addr}");
    Ok(())
}

fn log_connection(trigger: On<Add, Connected>, clients: Query<(), With<Client>>) {
    if clients.contains(trigger.entity) {
        info!("connected");
    }
}

/// A server that will not have this client says why; the client leaves.
fn hear_rejection(
    mut clients: Query<(Entity, &mut MessageReceiver<Rejected>), With<Client>>,
    mut farewell: ResMut<Farewell>,
    mut commands: Commands,
) {
    for (client, mut receiver) in &mut clients {
        if let Some(Rejected { reason }) = receiver.receive().last() {
            farewell.0 = Some(reason.to_string());
            commands.trigger(Disconnect { entity: client });
        }
    }
}

/// A connection attempt that ended without connecting.
fn leave_on_failure(
    trigger: On<Remove, Connecting>,
    session: Res<Session>,
    launched: Res<Launched>,
    clients: Query<&Disconnected, With<Client>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Ok(disconnected), Session::Join { server_addr, .. }) =
        (clients.get(trigger.entity), &*session)
    else {
        return;
    };
    warn!("could not connect: {}", disconnected.reason);
    leave(
        &format!("Could not reach the world at {server_addr}."),
        *launched,
        &mut exit,
    );
}

/// Losing an established connection to a remote world. A client also
/// starts out `Disconnected`, which is not a loss, and the host's own
/// client only disconnects as the app exits.
fn leave_on_disconnection(
    trigger: On<Remove, Connected>,
    session: Res<Session>,
    launched: Res<Launched>,
    farewell: Res<Farewell>,
    clients: Query<Option<&Disconnected>, With<Client>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Ok(disconnected), Session::Join { .. }) = (clients.get(trigger.entity), &*session) else {
        return;
    };
    let reason = disconnected.map_or(DisconnectedReason::Unknown, |d| d.reason.clone());
    warn!("disconnected: {reason}");
    let notice = farewell
        .0
        .clone()
        .unwrap_or_else(|| "The connection to the world was lost.".to_owned());
    leave(&notice, *launched, &mut exit);
}

/// Ends this instance, back to the title screen if it came from there.
fn leave(notice: &str, launched: Launched, exit: &mut MessageWriter<AppExit>) {
    if launched.from_title {
        launch::back_to_title(Some(notice));
        exit.write(AppExit::Success);
    } else {
        exit.write(AppExit::error());
    }
}
