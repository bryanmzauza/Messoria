# Hosting

A world runs on one machine, a player's game or a dedicated server, and
everyone else joins it by address over UDP. What follows about firewalls,
port forwarding and joining applies to both.

## Hosting from the game

**Host for friends** on the title screen asks for a world, a port (5717
unless changed) and the share of players that must be asleep to end the
night (50 percent unless changed), and starts the world open to guests.

Any world started from the title screen listens on its port from the
start, closed to guests. To let friends in without starting over, open the
game menu with `Esc` and choose **Open the world to friends on port 5717**.
A world opened this way ends the night only when everyone sleeps. If
another program, such as a dedicated server on the same machine, holds
port 5717, the world listens on the next free port and the menu names it.
A world hosted with **Host for friends** keeps the port it was given: if
that port is in use, the game goes back to the title screen and says so.

The game menu names everyone here, the host included. **Close the world**
sends every guest back to their title screen with "That world is closed to
guests."; **Back to the title screen** saves the world and ends it for
everyone. A hosted world takes 8 players at once, the host among them.
Guests' characters, fields and chests are kept in the host's save, ready
for the next time the host opens that world.

A world started from a terminal with `--world` listens on loopback only
and cannot be opened from the menu; add `--host` (and `--port`,
`--sleep-percent` if wanted) for guests.

## The firewall

The machine that hosts must accept UDP packets on the port.

**Windows.** The first time a world starts from the title screen, Windows
Defender Firewall asks whether Messoria may communicate on networks. Allow
it on private networks; also on public networks if Windows thinks your
home network is public. If the question was dismissed, add the rule from
an administrator terminal:

```bat
netsh advfirewall firewall add rule name="Messoria" dir=in action=allow protocol=UDP localport=5717
```

**Linux.** With ufw, `sudo ufw allow 5717/udp`; with firewalld,
`sudo firewall-cmd --permanent --add-port=5717/udp` and then
`sudo firewall-cmd --reload`.

Use your own port in place of 5717 if you changed it.

## Port forwarding

Players on the same network as the host join its local IP address
(`ipconfig` on Windows, `ip addr` on Linux; usually `192.168.x.x` or
`10.x.x.x`). Players elsewhere reach your router, not your machine, and
the router has to be told to pass the game's packets on:

1. Open the router's admin page in a browser: usually `192.168.0.1` or
   `192.168.1.1`, or the address on a label on the router.
2. Give the hosting machine a fixed local IP, often called a DHCP
   reservation, so the forward does not end up pointing at nothing when
   the address changes.
3. Find the port forwarding section (also called virtual servers, NAT or
   applications and gaming). Add a rule: protocol UDP, external port 5717,
   internal port 5717, internal IP the hosting machine's local IP.
4. Save, and give your friends your public IP, as a site like
   [whatismyip.com](https://www.whatismyip.com) shows it.

If the router's own internet address (on its status page) differs from
the one that site shows, your provider puts several customers behind one
address (carrier-grade NAT), and no forward on your router can work. A
dedicated server on a rented machine is the way around it for now.

The game does not open the port on the router by itself through UPnP.
Many routers have UPnP turned off, those that have it on open ports for
any program that asks, and a forward made by hand stays put and can be
checked. Access codes, planned after the MVP, will let friends join
without any of this (see the [roadmap](ROADMAP.md#after-the-mvp)).

## Joining by address

**Join a world** on the title screen, like `--connect` from a terminal,
takes an IP address or a host name, with or without a port:
`192.168.0.10`, `192.168.0.10:5800`, `farm.example.org` or
`farm.example.org:5800`. Without a port, 5717 is used. Host names must
have an IPv4 address. The addresses joined lately are listed under the
field.

## Dedicated server

`messoria-dedicated-server` runs a world with no window and no player of
its own. It comes in the release download and needs the `assets` folder
beside it.

| Flag | Default | |
|---|---|---|
| `--port <PORT>` | 5717 | UDP port to listen on |
| `--world <FOLDER>` | `saves/world` | Folder the world is saved in. The world saved there is resumed, or a new one is created |
| `--sleep-percent <PERCENT>` | 50 | Share of the players online, from 1 to 100, that must sleep to end the day |
| `--max-players <N>` | 16 | Players the world takes at once |
| `--start-time <HH:MM>` | 06:00 | Time of day a new world starts at, between 06:00 and 01:59; a saved world resumes at the time it was saved |
| `--minute-length <MS>` | | Real milliseconds one game minute lasts, to make days pass faster for testing; a game minute is a real second otherwise |

`--world` is relative to the working directory, not to the executable: run
the server from its own folder, or give an absolute path. The world is a
folder of files; back it up by copying the folder while the server is
stopped.

The server saves every five minutes, at every dawn and when it stops with
`Ctrl+C` (SIGINT). On Windows, closing its console window ends it without
saving: what changed since the last save, five minutes at most, is lost.
A save that cannot be read stops the server with the reason rather than
being overwritten.

### As a systemd service

Unpack the Linux download in `/opt/messoria`
(`sudo mkdir -p /opt/messoria`, then
`sudo tar -xzf messoria-<version>-linux-x64.tar.gz -C /opt/messoria --strip-components 1`),
make a `messoria` user
(`sudo useradd --system --home /opt/messoria --shell /usr/sbin/nologin messoria`),
give it the folder (`sudo chown -R messoria:messoria /opt/messoria`) and
write `/etc/systemd/system/messoria.service`:

```ini
[Unit]
Description=Messoria dedicated server
After=network-online.target
Wants=network-online.target

[Service]
User=messoria
Group=messoria
WorkingDirectory=/opt/messoria
ExecStart=/opt/messoria/messoria-dedicated-server --port 5717 --max-players 16
# Stop it as Ctrl+C does, so it saves on the way out.
KillSignal=SIGINT
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Start it with `sudo systemctl daemon-reload` and
`sudo systemctl enable --now messoria`, and follow its log with
`journalctl -u messoria -f`. The world is kept in
`/opt/messoria/saves/world`.

### Updating

Players and the server must run the same version. Stop the server
(`sudo systemctl stop messoria`), replace the executables and the `assets`
folder with the new release's, keep `saves/`, give the folder back to the
`messoria` user and start it again.

## What the server logs

On start it names the folder it saves in and the address it listens on;
after that, each save, each connection and disconnection, and each player
turned away, with the reason. Every minute it reports how it keeps up:

```
updates took 2.1 ms on average and 7.8 ms at worst, of a 33.3 ms tick; 96 columns of the valley are loaded for 3 players
```

The world advances 30 times a second, a tick every 33.3 ms. The average is
how long one update took over the last minute, and the worst is the
slowest. While both stay well under the tick, the server keeps up. A worst
time above the tick now and then is a hitch players may not notice; an
average near or above it means the machine cannot keep up with the
players it has. A column is a square of the valley about 32 m across; the
server loads the ones around each player and lets go of them as players
leave, so the count grows as players spread out.

## Turned away

A player who cannot come in is sent back to their title screen with one
of these:

- **That world runs another version of Messoria.** The server and the
  player run different releases; one of them has to update.
- **That world is full.** It has as many players as `--max-players`
  allows, or 8 for a world hosted from the game.
- **That world is closed to guests.** The host has not opened the world,
  or has closed it.

A player who cannot reach the world at all (a wrong address, a firewall,
a missing forward, the world not running) is told "Could not reach the
world at ...", and one whose connection drops, "The connection to the
world was lost."
