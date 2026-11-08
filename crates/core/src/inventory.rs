//! Inventory rules (Standard stage 7): movement kinds and their direction, the stock
//! balance after a movement (never negative), reorder checks and who an issue goes to.
//! Pure; the database layer applies them.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// What a stock movement does. Transfers are a pair (`transfer_out` at one location,
/// `transfer_in` at the other) that share a `transfer_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementKind {
    PurchaseIn,
    Issue,
    Return,
    WriteOff,
    AdjustIn,
    AdjustOut,
    TransferOut,
    TransferIn,
}

impl MovementKind {
    pub const ALL: &'static [MovementKind] = &[
        MovementKind::PurchaseIn,
        MovementKind::Issue,
        MovementKind::Return,
        MovementKind::WriteOff,
        MovementKind::AdjustIn,
        MovementKind::AdjustOut,
        MovementKind::TransferOut,
        MovementKind::TransferIn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MovementKind::PurchaseIn => "purchase_in",
            MovementKind::Issue => "issue",
            MovementKind::Return => "return",
            MovementKind::WriteOff => "write_off",
            MovementKind::AdjustIn => "adjust_in",
            MovementKind::AdjustOut => "adjust_out",
            MovementKind::TransferOut => "transfer_out",
            MovementKind::TransferIn => "transfer_in",
        }
    }

    pub fn parse(s: &str) -> Option<MovementKind> {
        MovementKind::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    /// Stock goes up (`true`) or down (`false`) at the movement's location.
    pub fn adds_stock(self) -> bool {
        matches!(
            self,
            MovementKind::PurchaseIn
                | MovementKind::Return
                | MovementKind::AdjustIn
                | MovementKind::TransferIn
        )
    }

    /// Issues and returns name who received or brought back the goods.
    pub fn needs_recipient(self) -> bool {
        matches!(self, MovementKind::Issue | MovementKind::Return)
    }
}

/// Who receives an issue (or brings a return).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientKind {
    Staff,
    Student,
    Room,
}

impl RecipientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RecipientKind::Staff => "staff",
            RecipientKind::Student => "student",
            RecipientKind::Room => "room",
        }
    }
    pub fn parse(s: &str) -> Option<RecipientKind> {
        match s {
            "staff" => Some(RecipientKind::Staff),
            "student" => Some(RecipientKind::Student),
            "room" => Some(RecipientKind::Room),
            _ => None,
        }
    }
}

/// The balance at a location after a movement; refused when it would go negative or the
/// quantity is not positive.
pub fn apply(
    on_hand: Decimal,
    kind: MovementKind,
    quantity: Decimal,
) -> Result<Decimal, &'static str> {
    if quantity <= Decimal::ZERO {
        return Err("the quantity must be more than zero");
    }
    if kind.adds_stock() {
        Ok(on_hand + quantity)
    } else if quantity > on_hand {
        Err("not enough stock at that location")
    } else {
        Ok(on_hand - quantity)
    }
}

/// An item needs reordering when its total on hand is at or below the reorder level.
pub fn below_reorder(on_hand: Decimal, reorder_level: Option<Decimal>) -> bool {
    reorder_level.is_some_and(|r| r > Decimal::ZERO && on_hand <= r)
}

/// Total cost of a movement when a unit cost is given.
pub fn total_cost(quantity: Decimal, unit_cost: Option<Decimal>) -> Option<Decimal> {
    unit_cost.map(|c| (c * quantity).round_dp(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(n: i64) -> Decimal {
        Decimal::from(n)
    }

    #[test]
    fn kinds_round_trip_and_know_their_direction() {
        for k in MovementKind::ALL {
            assert_eq!(MovementKind::parse(k.as_str()), Some(*k));
        }
        assert!(MovementKind::PurchaseIn.adds_stock());
        assert!(MovementKind::Return.adds_stock());
        assert!(!MovementKind::Issue.adds_stock());
        assert!(!MovementKind::WriteOff.adds_stock());
        assert!(MovementKind::Issue.needs_recipient());
        assert!(!MovementKind::PurchaseIn.needs_recipient());
        assert_eq!(RecipientKind::parse("room"), Some(RecipientKind::Room));
    }

    #[test]
    fn stock_never_goes_negative() {
        assert_eq!(apply(d(10), MovementKind::PurchaseIn, d(5)), Ok(d(15)));
        assert_eq!(apply(d(10), MovementKind::Issue, d(10)), Ok(d(0)));
        assert!(apply(d(10), MovementKind::Issue, d(11)).is_err());
        assert!(apply(d(10), MovementKind::WriteOff, d(0)).is_err());
        assert!(apply(d(0), MovementKind::TransferOut, d(1)).is_err());
        assert_eq!(apply(d(0), MovementKind::TransferIn, d(1)), Ok(d(1)));
    }

    #[test]
    fn reorder_and_cost() {
        assert!(below_reorder(d(5), Some(d(5))));
        assert!(below_reorder(d(0), Some(d(5))));
        assert!(!below_reorder(d(6), Some(d(5))));
        assert!(!below_reorder(d(0), None));
        assert!(!below_reorder(d(0), Some(d(0))));
        assert_eq!(
            total_cost(d(3), Some(Decimal::new(1250, 2))),
            Some(Decimal::new(3750, 2))
        );
        assert_eq!(total_cost(d(3), None), None);
    }
}
