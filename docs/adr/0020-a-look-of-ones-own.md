# 0020 — A look of one's own

**Status:** accepted. Replaces the looks picked from player ids in
[ADR 0012](0012-characters-village-and-icons.md).

## Context

Every character has been dressed from a look picked from its player's
id: the same on every screen, but nobody's choice. The MVP wants players
to make themselves a look on arrival, change clothes bought in the
village and have the barber do their hair, and to be seen so by everyone,
after a restart too. Hats are wanted, which the figure had no room for.

## Decision

- A look is server state: an `Appearance` component on the character,
  replicated like the rest and kept in the player's file (format 3). It
  names the skins the figure is laid from (body, outfit, hair, a hat if
  any) and the colors the eyes and the hair are tinted with; the eyes are
  a layer of their own, so a look has eyes of its choice. A player's name
  is chosen with it, kept the same way, and replaces the name made from
  their key wherever players are named.
- A player new to a world is told so, and the client answers with the
  look and name from its profile, if it has them; otherwise the character
  creator opens and stays until the player has a name and sets out. What
  is chosen goes into the profile, so the next world is joined as the
  same person. The server only takes a look the wardrobe offers newcomers
  (no hat), and only once per player; until then the character wears a
  look picked from its id and is asked again on its next arrival.
- Clothes are items: `Garment`s in the items file name the skin they
  paint and whether they are an outfit or a hat. The tailor sells them;
  the cabin comes with a wardrobe, where a garment in the backpack is
  changed into and what came off goes back into the backpack, on a lot
  the player may work. Nothing is worn from the backpack directly, so a
  change of clothes is a trip home.
- The barber restyles hair and its color from the wardrobe's choices for
  the village's price, at the counter in opening hours; the shop window
  shows hair instead of wares there.
- The figure gains a cap and a brim box on the head, large enough to
  swallow the crown and the top tufts of every hairstyle, so a hat covers
  hair instead of being pierced by it. Every skin was repainted for the
  new unwrap.
- The client has a looking glass: a figure dressed in whatever a window
  wants shown, drawn by its own camera into an image the creator, the
  wardrobe and the barber's counter put on screen, turning slowly.

## Consequences

- Looks and names travel with the save, not the client, so a player is
  seen the same from every machine; the profile only seeds new worlds.
- A look the content no longer has (a removed skin) falls back to the one
  picked from the id, without touching the file, so content can change
  under old saves.
- Bots introduce themselves too, so load tests exercise the same path.
- The creator takes typed text, which the game's own keys must yield to;
  panels now say when they take typing.
