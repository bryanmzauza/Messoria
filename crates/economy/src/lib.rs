//! Money, and market prices that respond to supply.
//!
//! Every player has a [`Wallet`]. Shops sell at fixed prices, but what they
//! pay for goods moves with the server-wide [`Market`]: each unit anyone
//! sells saturates the market for that item and lowers its price, and the
//! saturation fades every night. Produce is worth more out of season, better
//! harvests are worth more, and a [`SalesLedger`] caps how much of each item
//! one player can sell a shop in a day. [`sell`] and [`buy`] carry out a trade
//! at a shop's counter, or say why it cannot happen.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod ledger;
mod market;
mod trade;
mod wallet;

pub use crate::{
    ledger::SalesLedger,
    market::{Market, PriceBasis},
    trade::{Customer, Refusal, buy, sell},
    wallet::{MoneyError, Wallet},
};
