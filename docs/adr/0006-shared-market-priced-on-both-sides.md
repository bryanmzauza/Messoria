# 0006 — One shared market, priced by the same rules on server and client

**Status:** accepted

## Context

Prices must respond to what the whole server sells, so that fifty players
flooding the market with one crop drive its price down for everyone. Players
also need to see what a shop will pay before they sell, and the price they
see must be the price they get. The server is authoritative
([ADR 0001](0001-server-authoritative-single-codebase.md)), and the rules
that set prices belong in a pure crate ([ADR 0003](0003-pure-domain-crates.md)).

## Decision

- `messoria-economy` holds wallets, the market, each player's daily sales and
  the trade rules. `sell` and `buy` carry out a trade completely or return
  why it cannot happen.
- The market keeps a saturation per item: every unit sold adds one, and each
  night a share fades. A unit sells for its seasonal, quality-adjusted price
  divided by `1 + saturation / halves_after`. Produce is worth more outside
  the seasons its crop grows in, which comes from the crop data rather than
  a separate price table. The tuning lives in `shops.ron`.
- Each player can sell a shop a limited number of units of each item a day.
  The limit is per player, so nobody can use up a shop's quota for everyone
  else, while saturation remains shared by the whole server.
- The market is replicated to clients as one component. Clients call the same
  trade functions on copies of their state to price every button and to say
  why a deal is unavailable. The server calls them on the real state.

## Consequences

- What a shop window offers is exactly what the trade pays, without a
  request and reply for every quote.
- Selling in bulk pays the same as selling one unit at a time, so there is
  nothing to gain by splitting or grouping sales.
- Price curves can be checked in unit tests against the shipped tuning,
  including simulated days of many players selling.
- The market component grows with the number of items sold lately, not with
  the number of players. Items whose saturation fades away drop out of it.
- Money uses whole coins in `u32`. A trade that would overflow a wallet is
  refused rather than clamped.
