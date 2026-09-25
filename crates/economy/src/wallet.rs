//! Players' money.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The money one player has, in whole coins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    coins: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MoneyError {
    #[error("not enough money")]
    Insufficient,
    #[error("that much money does not fit in a wallet")]
    Overflow,
}

impl Wallet {
    pub fn with(coins: u32) -> Self {
        Self { coins }
    }

    pub fn coins(self) -> u32 {
        self.coins
    }

    /// Takes `amount` out, or nothing if there is not that much.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Insufficient`] if the wallet holds less than `amount`.
    pub fn pay(&mut self, amount: u32) -> Result<(), MoneyError> {
        self.coins = self
            .coins
            .checked_sub(amount)
            .ok_or(MoneyError::Insufficient)?;
        Ok(())
    }

    /// Puts `amount` in, or nothing if it would not fit.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the wallet cannot hold that much.
    pub fn receive(&mut self, amount: u32) -> Result<(), MoneyError> {
        self.coins = self.coins.checked_add(amount).ok_or(MoneyError::Overflow)?;
        Ok(())
    }

    /// Moves `amount` from this wallet into `to`, completely or not at all.
    ///
    /// # Errors
    ///
    /// If this wallet holds less than `amount` or `to` cannot hold it.
    pub fn transfer(&mut self, to: &mut Self, amount: u32) -> Result<(), MoneyError> {
        let (mut from_after, mut to_after) = (*self, *to);
        from_after.pay(amount)?;
        to_after.receive(amount)?;
        (*self, *to) = (from_after, to_after);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paying_needs_enough_money() {
        let mut wallet = Wallet::with(50);
        assert_eq!(wallet.pay(20), Ok(()));
        assert_eq!(wallet.pay(31), Err(MoneyError::Insufficient));
        assert_eq!(wallet.coins(), 30);
    }

    #[test]
    fn transfers_move_all_the_money_or_none() {
        let (mut alice, mut bob) = (Wallet::with(100), Wallet::with(5));
        assert_eq!(alice.transfer(&mut bob, 40), Ok(()));
        assert_eq!((alice.coins(), bob.coins()), (60, 45));

        assert_eq!(alice.transfer(&mut bob, 61), Err(MoneyError::Insufficient));
        let mut full = Wallet::with(u32::MAX);
        assert_eq!(alice.transfer(&mut full, 1), Err(MoneyError::Overflow));
        assert_eq!((alice.coins(), bob.coins()), (60, 45));
    }
}
