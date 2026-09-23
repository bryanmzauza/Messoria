//! How peers find and authenticate each other.
//!
//! All transport configuration lives here so the client, the server and the
//! load-testing bots cannot drift apart.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

use bevy::prelude::*;
use lightyear::{
    netcode::{self, NetcodeClient, NetcodeServer},
    prelude::{
        client::NetcodeConfig as ClientNetcodeConfig,
        server::{NetcodeConfig as ServerNetcodeConfig, Server, ServerUdpIo},
        *,
    },
};

/// UDP port a server listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 5717;

/// Identifies this wire protocol. Builds with different values refuse to
/// connect to each other, so bump it on every incompatible protocol change.
const PROTOCOL_ID: u64 = 1;

/// Key that signs connect tokens.
///
/// Clients currently build their own tokens, which means every client holds
/// this key and it protects nothing; it is public on purpose. Tokens will be
/// issued by the rendezvous service instead (roadmap M8), at which point the
/// key becomes a server-side secret.
const CONNECT_TOKEN_KEY: [u8; 32] = [0; 32];

/// Seconds of silence after which either side drops the connection.
const TIMEOUT_SECS: i32 = 5;

/// Which side of the connection an app plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkRole {
    /// Dedicated server: simulates the world and accepts remote clients.
    Server,
    /// Client of a remote server.
    Client,
    /// A server and a client in the same app, for a player hosting their own world.
    Host,
}

/// Resolves a server address typed by a player: an IP or host name, with an
/// optional port that defaults to [`DEFAULT_PORT`].
///
/// # Errors
///
/// Fails if a host name cannot be resolved to an IPv4 address.
pub fn resolve_server_addr(input: &str) -> io::Result<SocketAddr> {
    if let Ok(addr) = input.parse::<SocketAddr>() {
        return Ok(addr);
    }
    if let Ok(ip) = input.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, DEFAULT_PORT));
    }

    let with_port = if input.contains(':') {
        input.to_owned()
    } else {
        format!("{input}:{DEFAULT_PORT}")
    };
    // Clients bind an IPv4 socket, so an IPv6 result would be unreachable.
    with_port
        .to_socket_addrs()?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no IPv4 address found for {input}"),
            )
        })
}

/// Components for a server listening for clients on `bind_addr`.
pub fn server_listener(bind_addr: SocketAddr) -> impl Bundle {
    (
        Name::new("Server"),
        Server::default(),
        NetcodeServer::new(ServerNetcodeConfig {
            protocol_id: PROTOCOL_ID,
            private_key: CONNECT_TOKEN_KEY,
            client_timeout_secs: TIMEOUT_SECS,
            // Self-issued tokens carry whatever address the client dialed (a LAN
            // or public IP), which the server cannot know from its bind address.
            // Checking it only adds value once tokens come from a trusted issuer.
            server_addr_check: false,
            ..default()
        }),
        LocalAddr(bind_addr),
        ServerUdpIo::default(),
    )
}

/// Components for a client connecting to the server at `server_addr`.
///
/// `simulated_latency` delays every incoming packet by that amount, which is
/// how latency is exercised during development.
///
/// # Errors
///
/// Fails if a connect token cannot be generated for `server_addr`.
pub fn remote_client(
    server_addr: SocketAddr,
    client_id: u64,
    simulated_latency: Option<Duration>,
) -> Result<impl Bundle, netcode::Error> {
    let authentication = Authentication::Manual {
        server_addr,
        client_id,
        private_key: CONNECT_TOKEN_KEY,
        protocol_id: PROTOCOL_ID,
    };
    let netcode = NetcodeClient::new(
        authentication,
        ClientNetcodeConfig {
            client_timeout_secs: TIMEOUT_SECS,
            ..default()
        },
    )?;
    let conditioner = simulated_latency.map(|latency| {
        RecvLinkConditioner::new(LinkConditionerConfig {
            incoming_latency: latency,
            ..default()
        })
    });

    Ok((
        Name::new("Client"),
        Client,
        ReplicationReceiver,
        Link::default().with_conditioner(conditioner),
        LocalAddr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))),
        PeerAddr(server_addr),
        netcode,
        UdpIo::default(),
    ))
}

/// Components for the client of a player hosting `server` in the same app.
/// No packets are exchanged; lightyear routes everything in memory.
pub fn host_client(server: Entity) -> impl Bundle {
    (Name::new("Host client"), Client, LinkOf { server })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_without_port_uses_default_port() {
        let addr = resolve_server_addr("192.168.0.10").unwrap();
        assert_eq!(addr, SocketAddr::from(([192, 168, 0, 10], DEFAULT_PORT)));
    }

    #[test]
    fn explicit_port_is_kept() {
        let addr = resolve_server_addr("10.0.0.2:4000").unwrap();
        assert_eq!(addr, SocketAddr::from(([10, 0, 0, 2], 4000)));
    }

    #[test]
    fn host_names_are_resolved() {
        let addr = resolve_server_addr("localhost").unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), DEFAULT_PORT);
    }
}
