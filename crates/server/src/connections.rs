//! Listening for clients and preparing their connections.

use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
use messoria_shared::{network, tick};

pub(crate) struct ConnectionsPlugin {
    pub bind_addr: SocketAddr,
}

impl Plugin for ConnectionsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BindAddr(self.bind_addr))
            // A hosted world runs at the display rate; there is no point in
            // sending state more often than the simulation changes it.
            .insert_resource(ReplicationMetadata::new(tick::tick_duration()))
            .add_systems(Startup, start_listening)
            .add_observer(log_listening)
            .add_observer(enable_replication)
            .add_observer(log_connection)
            .add_observer(log_disconnection);
    }
}

#[derive(Resource)]
struct BindAddr(SocketAddr);

fn start_listening(bind_addr: Res<BindAddr>, mut commands: Commands) {
    let server = commands.spawn(network::server_listener(bind_addr.0)).id();
    commands.trigger(Start { entity: server });
}

fn log_listening(trigger: On<Add, Started>, addrs: Query<&LocalAddr>) {
    if let Ok(addr) = addrs.get(trigger.entity) {
        info!("listening on {}", addr.0);
    }
}

/// Every client receives the replicated world.
fn enable_replication(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

fn log_connection(trigger: On<Add, Connected>, clients: Query<&RemoteId, With<ClientOf>>) {
    if let Ok(peer) = clients.get(trigger.entity) {
        info!("{:?} connected", peer.0);
    }
}

fn log_disconnection(trigger: On<Remove, Connected>, clients: Query<&RemoteId, With<ClientOf>>) {
    if let Ok(peer) = clients.get(trigger.entity) {
        info!("{:?} disconnected", peer.0);
    }
}
