//! How players are known: the key that names a player's file and that
//! lots and invitations refer to, and the name shown for a player until
//! players choose names of their own.

use lightyear::prelude::PeerId;

/// The key naming a player, the same every time they join: the id in their
/// profile, or `host` for the player hosting the world from the game.
pub fn player_key(peer: PeerId) -> Option<String> {
    match peer {
        PeerId::Netcode(id) => Some(format!("{id:016x}")),
        PeerId::Local(_) => Some("host".to_owned()),
        PeerId::Entity(_) | PeerId::Raw(_) | PeerId::Steam(_) | PeerId::Server => None,
    }
}

/// How the player `key` names is called on screen.
pub fn player_name(key: &str) -> String {
    if key == "host" {
        return "Host".to_owned();
    }
    let tail = key.get(key.len().saturating_sub(4)..).unwrap_or(key);
    format!("Player {}", tail.to_uppercase())
}

/// How the player behind `peer` is called on screen.
pub fn name_of(peer: PeerId) -> String {
    player_key(peer).map_or_else(|| "Someone".to_owned(), |key| player_name(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_files_and_names_name_players() {
        let key = player_key(PeerId::Netcode(0xabcd)).expect("a key");
        assert_eq!(key, "000000000000abcd");
        assert_eq!(player_name(&key), "Player ABCD");
        assert_eq!(name_of(PeerId::Local(0)), "Host");
        assert_eq!(player_key(PeerId::Server), None);
    }
}
