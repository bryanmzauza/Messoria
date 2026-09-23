//! Joining a remote server or the world hosted by this same process.

use std::{net::SocketAddr, time::Duration};

use bevy::prelude::*;
use lightyear::prelude::{server::Server, *};
use messoria_shared::network;

/// Where the client's world lives.
#[derive(Clone, Debug)]
pub enum Session {
    /// The world is hosted in this process by `ServerPlugin`.
    Host,
    /// The world lives on another machine.
    Join {
        server_addr: SocketAddr,
        /// Extra delay applied to incoming packets, to exercise latency locally.
        simulated_latency: Option<Duration>,
    },
}

pub(crate) struct ConnectionPlugin {
    pub session: Session,
}

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        match self.session.clone() {
            // The in-process server entity is spawned during `Startup`.
            Session::Host => app.add_systems(PostStartup, join_hosted_world),
            Session::Join {
                server_addr,
                simulated_latency,
            } => app.add_systems(Startup, move |commands: Commands| -> Result {
                join_remote_world(commands, server_addr, simulated_latency)
            }),
        };

        app.add_observer(log_connection)
            .add_observer(log_connection_failure)
            .add_observer(log_disconnection);
    }
}

fn join_hosted_world(server: Single<Entity, With<Server>>, mut commands: Commands) {
    let client = commands.spawn(network::host_client(*server)).id();
    commands.trigger(Connect { entity: client });
}

fn join_remote_world(
    mut commands: Commands,
    server_addr: SocketAddr,
    simulated_latency: Option<Duration>,
) -> Result {
    let client_id = rand::random();
    let client = commands
        .spawn(network::remote_client(
            server_addr,
            client_id,
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

/// Reports a connection attempt that ended without connecting.
fn log_connection_failure(
    trigger: On<Remove, Connecting>,
    clients: Query<&Disconnected, With<Client>>,
) {
    if let Ok(disconnected) = clients.get(trigger.entity) {
        warn!("could not connect: {}", disconnected.reason);
    }
}

/// Reacts to losing an established connection only: a client also starts out
/// `Disconnected`, which is not worth reporting.
fn log_disconnection(
    trigger: On<Remove, Connected>,
    clients: Query<Option<&Disconnected>, With<Client>>,
) {
    if let Ok(disconnected) = clients.get(trigger.entity) {
        let reason = disconnected.map_or(DisconnectedReason::Unknown, |d| d.reason.clone());
        warn!("disconnected: {reason}");
    }
}
