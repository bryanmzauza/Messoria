# Messoria — Game Design Document

> Living document. Anything marked **[OPEN]** still needs a decision.
> Engine: **Rust + Bevy**. Pinned versions live in the workspace `Cargo.toml`.

---

## 1. Vision

**Name:** *Messoria* — from Latin *messoria*, "of the harvest" (from *messor*, the reaper). It reads like the name of a kingdom: "the land of the harvest". *Messor* is also the genus of harvester ants, a possible emblem for the village.

**Pitch:** *Messoria* is a cooperative 3D farming sim set in a fantasy world. It has the pace and warmth of Stardew Valley and the freedom of a shapeable voxel world in the style of Minecraft, rendered as smooth low-poly terrain rather than cubes.

**Pillars (every feature must serve at least one):**

1. **Cozy routine** — every day has a rhythm: plant, tend, explore, sleep.
2. **Shapeable world** — players dig, level, build and reshape the terrain.
3. **Better together** — playing with friends is the primary mode. Opening a world and inviting friends takes under a minute; large servers host whole communities.
4. **Living economy** — prices react to what players do; nobody gets rich planting the same crop forever.

**Platform:** PC (Windows and Linux first; macOS later).

**Setting:** fantasy village. **[OPEN]** sub-theme (cozy medieval fantasy? elven? light steampunk?).

---

## 2. Gameplay loop

**Daily:** wake up → water and harvest → carry produce to the village and sell it at the shops (before it spoils) → explore the forest and mines → talk to villagers → return home → sleep.

**Seasonal:** each season changes available crops, weather, prices and resources.

**Long term:** expand the farm, upgrade tools, unlock areas, build friendships, grow wealth.

---

## 3. World

### 3.1 Terrain
- Stored as a **voxel grid** with a material per cell (soil, grass, stone, sand, ore…).
- Rendered with **Surface Nets** → organic hills, no cubes. Flat-shaded low-poly facets.
- Split into **chunks** (e.g. 32³) loaded based on player proximity.

### 3.2 Areas
| Area | Generation | Deformable? | Purpose |
|---|---|---|---|
| Village | **Fixed** (hand-made) | No (protected) | Villagers, shops, events |
| Farms | **Fixed** | Yes | Planting, building, terraforming |
| Forest | **Procedural** (world seed) | Partially | Gathering, wood, foraging |
| Mines | **Procedural** (floors) | Yes | Mining, ores, hazards |

- With individual money and 50+ players, each player (or group) needs **their own land**: the farm region is divided into **plots** that players claim or buy. **[OPEN]** number and size of plots, and whether friends can share a plot.

---

## 4. Time and seasons

- **1 in-game day = 20 real minutes:** a day runs from 06:00 to 02:00, one game minute per real second.
- **365-day year**, split into 4 seasons (91/91/91/92 days).
- A full year ≈ **122 real hours**, so the game is long-form; crop cycles must be proportional (several harvests per season, perennials, trees that take seasons to bear fruit).
- Day/night cycle with dynamic lighting; shops have opening hours and closed days.
- Weather: sun, rain (waters crops), storms, snow in winter.

### 4.1 Advancing the day in multiplayer
Waiting for everyone to sleep does not work with 50 players:
- **Solo:** sleeping skips to morning.
- **Small server (hosted in-game):** the day advances when **all** players sleep (Stardew style).
- **Dedicated server:** time runs continuously; sleeping skips the night if **X% of online players** are asleep (configurable, 50% by default, as in Minecraft).
- Players may go to sleep from 18:00. Anyone still awake at 2 AM passes out and the day ends for everyone; passing out restores only half the energy that sleeping would.

---

## 5. Systems

### 5.1 Farming
- Hoe tills the soil → seed → water daily → grows through stages → harvest.
- Fields are one-meter squares on a grid. Only grass or soil on gentle slopes can be tilled; digging or raising the ground under a field destroys it.
- Each crop is defined in a data file (RON): seasons, days per stage, whether it regrows, how much it yields. Growth counts watered days only, resolved each night.
- Harvest quality (normal / silver / gold) is influenced by fertilizer: compost, which is what spoiled food becomes. Fertilizer feeds one harvest.
- Crops out of season die when the season changes.
- Rain waters every field for the day.

### 5.2 Tools
Hoe, watering can, axe, pickaxe, shovel (terraforming), scythe, basic weapon. Upgrade tiers (copper → iron → gold) increase area and efficiency.

### 5.3 Inventory and spoilage
- Hotbar (10 slots) + expandable backpack (20 slots to start), chests for storage. The held hotbar item decides what the mouse buttons do.
- **Perishable items expire:** each item has a shelf life in in-game days; spoiled items become trash or compost. Stacking perishables averages their freshness by count.
- Digging with the shovel yields the dug material (soil, stone, sand); raising the ground uses soil.
- The shovel moves the ground in a small circle down or up to the next level; levels are half a meter apart and shared by the whole world, so neighboring digs join into flat ground and digging along a slope cuts terraces.
- Preservation: **[OPEN]** fridge/cellar, and processing (jam, pickles, wine) to extend shelf life and add value.

### 5.4 Energy
Actions consume energy; food restores it; sleep refills it. A character has 100 energy; each shovel use costs 2, and tools cannot be used without enough energy left.

### 5.5 Mining and combat
- Mines with procedural floors, ores by depth, stairs/elevator every N floors.
- Combat: enemies in the mines and in the forest at night, simple melee weapon. **Not part of the MVP** — it ships right after, together with the mines.

### 5.6 Crafting and building
- Workbench for items (fences, chests, sprinklers, furnace).
- Larger buildings (barn, coop, house) placed on the player's plot.

### 5.7 Villagers and relationships
- Villagers follow daily schedules (by hour and season).
- Friendship through conversation and gifts; events unlock at friendship levels.
- Friendship is **per player**.

### 5.8 Economy
**Selling happens only at shops** — there is no shipping bin. Players carry produce to the merchant and are paid immediately. Each shop buys specific categories (grocer: crops; blacksmith: ores; carpenter: wood). Shops keep opening hours.

**Dynamic price per item (per server):**
- **Seasonal base price** — the same item is worth more outside the season in which it is abundant.
- **Supply and demand** — every sale raises the item's "saturation" and lowers its price; saturation recovers gradually each day.
- **Daily limit per shop** — each shop buys at most N units of an item per day (**[OPEN]** per player or shared across the shop).
- Because the economy is **shared by the whole server**, 50 players planting the same crop drive the price down for everyone → encourages diversification and specialization.

**Individual money:**
- Each player has their own wallet.
- Players can **transfer** and **lend** money to each other.
- **Loans** are tracked by the system: amount, due date, optional interest agreed by both parties. **[OPEN]** what happens on default (reputation mark only? automatic collection from the wallet?).

### 5.9 Future (post-MVP)
Fishing, animals, cooking/processing, seasonal festivals, marriage, collections, direct player-to-player trading.

---

## 6. Multiplayer

### 6.1 Two modes, one codebase

| | **Hosted world** | **Dedicated server** |
|---|---|---|
| Audience | Group of friends | Communities |
| Players | up to **8** | **50+** |
| How it starts | "Open to friends" button in-game | Headless binary on a VPS |
| How players join | **Access code** (e.g. `FAZ-7K2Q`) | IP/domain or code |
| Time | Skips when everyone sleeps | Continuous, % sleeping |

Both run the **same authoritative server**; in hosted mode it simply runs inside the game process.

### 6.2 Access codes (fast and easy)
A player hosting from home is usually behind NAT, so friends cannot connect directly. Solution:
- A lightweight **rendezvous service** running on our own VPS: the hosting game registers and receives a short code; whoever types the code learns how to connect.
- Try a **direct connection** (UDP hole punching); fall back to a **relay** through the VPS.
- No router or port configuration for players.
- Future option: if the game ships on Steam, use Steam networking/relay and friend-list invites.

### 6.3 Dedicated server (50+ players)
- Headless, no graphics, configured by file (player limit, sleep percentage, economy rules, whitelist).
- **Interest management:** each client receives only nearby entities and chunks.
- Terrain: a full chunk when it enters range, then only voxel **deltas**.
- Crop simulation **batched and event-driven** (growth resolved at day rollover, not every frame) to handle thousands of crops.
- Fixed server tick (e.g. 20–30 Hz); villagers and areas far from players simulated at low frequency.
- Performance targets validated with load tests (simulated bots) from early on.

### 6.4 Network model
- **Authoritative server:** all logic (time, crops, terrain, villagers, economy, combat) runs on the server.
- **Client:** sends inputs, receives state, renders. Movement and camera use **client-side prediction**.
- Replication with **lightyear** (see [ADR 0002](../adr/0002-networking-lightyear.md)).
- World saves **on the server** (world + one file per player); periodic autosave.
- Permissions: owner/admin can kick, ban, protect areas and change settings.
- **Multiplayer from the first prototype.**

---

## 7. Art and audio direction

- Stylized low-poly fantasy, warm colors, soft shadows, distance fog.
- Base assets: CC0 packs (Kenney, Quaternius), plus fantasy packs for the village and characters.
- Crop growth stages generated in code.
- Calm music per season.

---

## 8. Camera and controls

- **First and third person, toggled with a key** (like F5 in Minecraft).
- Crosshair at the center of the screen; tools act on the targeted voxel/tile, with a **visual highlight** of the target (essential for precise planting).
- Keyboard + mouse first; gamepad later.

---

## 9. Code architecture

See [architecture.md](../architecture.md) for the crate layout and dependency rules.

Principles:
- **One Bevy plugin per game system.**
- Pure logic (growth, economy, inventory, spoilage) lives in functions testable without launching the game.
- Content lives in data files, never hardcoded.

---

## 10. MVP scope (vertical slice)

- [ ] Headless server + client connecting by IP
- [ ] "Open to friends" with access code (rendezvous + relay)
- [ ] Smooth voxel farm terrain, deformable with the shovel
- [ ] Movement, first/third-person camera, players see each other
- [ ] Day cycle and sleep rules
- [ ] 5 crops: till, plant, water, grow, harvest
- [ ] Inventory + hotbar, items with shelf life
- [ ] 1 village shop: buys crops (dynamic price + daily limit) and sells seeds
- [ ] Individual wallet and transfers between players
- [ ] Server-side save/load
- [ ] Load test with bots

**Success criteria:** 2–8 friends play one in-game week together through an access code and want to keep going; the dedicated server handles 50 bots without degrading.

The milestone breakdown lives in [ROADMAP.md](../ROADMAP.md).

---

## 11. Out of MVP

Combat and mines, fishing, animals, loans (right after the MVP), festivals, marriage, modding, mobile/console, accounts.

---

## 12. Open decisions

1. Fantasy sub-theme
2. Farm plots: number, size, sharing
3. Sales limit per player or shared across the shop
4. Loan defaults
5. Item preservation (fridge, processing)

### Resolved
- **Combat in the MVP:** no; it ships right after, together with the mines.
- **Networking library:** lightyear ([ADR 0002](../adr/0002-networking-lightyear.md)).
