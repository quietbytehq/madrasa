//! Fee arithmetic. Pure, exact decimals (`rust_decimal`), rounded half away from zero to
//! two places at every boundary that becomes a stored amount.
//!
//! Terms:
//! - **gross**: sum of the voucher's charge lines for the month.
//! - **sibling discount**: percentage of gross, for students who qualify under the policy.
//! - **concession**: another discount (staff child, scholarship, custom), percent or fixed.
//! - **net monthly**: gross − all discounts (never below zero).
//! - **arrears**: unpaid balance carried forward from earlier vouchers (and years).
//! - **total due**: net monthly + arrears.
//! - **late fee**: percentage of net monthly, assessed when the voucher is unpaid after the
//!   due date, unless waived.

use chrono::{Datelike, NaiveDate};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

/// Round to currency precision (2 dp, half away from zero).
pub fn money(d: Decimal) -> Decimal {
    d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// `percent` % of `base`, rounded to money.
pub fn percent_of(base: Decimal, percent: Decimal) -> Decimal {
    money(base * percent / Decimal::ONE_HUNDRED)
}

/// Which siblings receive the sibling discount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SiblingRule {
    /// Every student who has at least one sibling enrolled.
    #[default]
    All,
    /// All but the first (eldest / earliest admitted) child of the family.
    SecondChildOnwards,
}

/// School-wide fee policy (stored in settings as `fees.policy`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeePolicy {
    /// Percent of the total monthly fee, e.g. 10.
    pub sibling_discount_percent: Decimal,
    pub sibling_rule: SiblingRule,
    /// Percent of the net monthly fee charged after the due date, e.g. 10.
    pub late_fee_percent: Decimal,
    /// Day of the billing month the voucher is due (late fee applies after it), e.g. 15.
    pub due_day: u32,
}

impl Default for FeePolicy {
    fn default() -> Self {
        Self {
            sibling_discount_percent: Decimal::new(10, 0),
            sibling_rule: SiblingRule::All,
            late_fee_percent: Decimal::new(10, 0),
            due_day: 15,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("percentages must be between 0 and 100")]
    Percent,
    #[error("due day must be between 1 and 28")]
    DueDay,
}

impl FeePolicy {
    pub fn validate(&self) -> Result<(), PolicyError> {
        let ok = |p: Decimal| p >= Decimal::ZERO && p <= Decimal::ONE_HUNDRED;
        if !ok(self.sibling_discount_percent) || !ok(self.late_fee_percent) {
            return Err(PolicyError::Percent);
        }
        if !(1..=28).contains(&self.due_day) {
            return Err(PolicyError::DueDay);
        }
        Ok(())
    }

    /// Due date for a billing month (first day of month given).
    pub fn due_date(&self, billing_month: NaiveDate) -> NaiveDate {
        NaiveDate::from_ymd_opt(billing_month.year(), billing_month.month(), self.due_day)
            .unwrap_or(billing_month)
    }

    /// Does a student with `sibling_count` enrolled siblings, ranked `rank` (0 = first child)
    /// within the family, get the sibling discount?
    pub fn sibling_discount_applies(&self, sibling_count: usize, rank: usize) -> bool {
        if sibling_count == 0 || self.sibling_discount_percent.is_zero() {
            return false;
        }
        match self.sibling_rule {
            SiblingRule::All => true,
            SiblingRule::SecondChildOnwards => rank > 0,
        }
    }
}

/// A concession granted to a student: either a percent of gross or a fixed amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcessionTerms {
    pub percent: Option<Decimal>,
    pub amount: Option<Decimal>,
}

impl ConcessionTerms {
    pub fn value_on(&self, gross: Decimal) -> Decimal {
        let mut v = Decimal::ZERO;
        if let Some(p) = self.percent {
            v += percent_of(gross, p);
        }
        if let Some(a) = self.amount {
            v += money(a);
        }
        v
    }
}

/// Inputs for one voucher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherInput {
    /// Charge lines (head, amount).
    pub lines: Vec<(String, Decimal)>,
    pub sibling_discount: bool,
    pub concessions: Vec<ConcessionTerms>,
    /// Outstanding balance from earlier vouchers (may be zero or negative = credit).
    pub arrears: Decimal,
}

/// Computed amounts stored on the voucher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoucherTotals {
    pub gross: Decimal,
    pub sibling_discount: Decimal,
    pub concession_discount: Decimal,
    pub net_monthly: Decimal,
    pub arrears: Decimal,
    pub total_due: Decimal,
}

/// Compute a voucher. Discounts are each a share of gross, and together never exceed it.
pub fn compute_voucher(policy: &FeePolicy, input: &VoucherInput) -> VoucherTotals {
    let gross = money(input.lines.iter().map(|(_, a)| *a).sum::<Decimal>());
    let sibling = if input.sibling_discount {
        percent_of(gross, policy.sibling_discount_percent)
    } else {
        Decimal::ZERO
    };
    let mut concession = money(input.concessions.iter().map(|c| c.value_on(gross)).sum());
    if sibling + concession > gross {
        concession = gross - sibling;
    }
    let net_monthly = gross - sibling - concession;
    let arrears = money(input.arrears);
    VoucherTotals {
        gross,
        sibling_discount: sibling,
        concession_discount: concession,
        net_monthly,
        arrears,
        total_due: net_monthly + arrears,
    }
}

/// Late fee for a voucher that is still (partly) unpaid on `as_of`: a percentage of the net
/// monthly fee once the due date has passed. Zero on or before the due date, or when waived.
pub fn late_fee(
    policy: &FeePolicy,
    net_monthly: Decimal,
    due_date: NaiveDate,
    as_of: NaiveDate,
    waived: bool,
) -> Decimal {
    if waived || as_of <= due_date {
        return Decimal::ZERO;
    }
    percent_of(net_monthly, policy.late_fee_percent)
}

/// Outstanding balance of a voucher: total due + assessed late fee − net payments.
pub fn voucher_balance(total_due: Decimal, late_fee: Decimal, net_paid: Decimal) -> Decimal {
    money(total_due + late_fee - net_paid)
}

/// Voucher status from its balance and payments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoucherStatus {
    Unpaid,
    Partial,
    Paid,
    /// Balance was carried forward into a later voucher; no longer payable itself.
    Carried,
    Cancelled,
}

impl VoucherStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VoucherStatus::Unpaid => "unpaid",
            VoucherStatus::Partial => "partial",
            VoucherStatus::Paid => "paid",
            VoucherStatus::Carried => "carried",
            VoucherStatus::Cancelled => "cancelled",
        }
    }

    pub fn derive(balance: Decimal, net_paid: Decimal) -> VoucherStatus {
        if balance <= Decimal::ZERO {
            VoucherStatus::Paid
        } else if net_paid > Decimal::ZERO {
            VoucherStatus::Partial
        } else {
            VoucherStatus::Unpaid
        }
    }
}

/// Running ledger totals for a student.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTotals {
    pub billed: Decimal,
    pub discounts: Decimal,
    pub fines: Decimal,
    pub paid: Decimal,
    pub balance: Decimal,
}

/// One voucher's contribution to the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub gross: Decimal,
    pub discounts: Decimal,
    pub late_fee: Decimal,
    pub net_paid: Decimal,
}

/// Sum ledger entries. Arrears are not re-billed: each voucher counts its own gross once, so the
/// balance equals Σ(gross − discounts + late fee − paid) regardless of carry-forward.
pub fn ledger_totals(entries: &[LedgerEntry]) -> LedgerTotals {
    let mut t = LedgerTotals::default();
    for e in entries {
        t.billed += e.gross;
        t.discounts += e.discounts;
        t.fines += e.late_fee;
        t.paid += e.net_paid;
    }
    t.balance = money(t.billed - t.discounts + t.fines - t.paid);
    t
}

/// Carry-forward into the next voucher: the student's current outstanding balance (sum of
/// voucher balances); credits (overpayments) carry as negative arrears.
pub fn carry_forward(outstanding_balances: &[Decimal]) -> Decimal {
    money(outstanding_balances.iter().copied().sum())
}

/// First day of the month `n` months after `month` (first-of-month date).
pub fn add_months(month: NaiveDate, n: i32) -> NaiveDate {
    let total = month.year() * 12 + month.month0() as i32 + n;
    NaiveDate::from_ymd_opt(total.div_euclid(12), (total.rem_euclid(12) + 1) as u32, 1)
        .expect("valid month")
}

/// Billing months of an academic year (first-of-month dates), inclusive.
pub fn billing_months(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut m = NaiveDate::from_ymd_opt(start.year(), start.month(), 1).expect("valid");
    while m <= end {
        out.push(m);
        m = add_months(m, 1);
    }
    out
}

/// Payment methods accepted at the counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Bank,
    Online,
    OneBill,
    Kuickpay,
}

impl PaymentMethod {
    pub const ALL: &'static [PaymentMethod] = &[
        PaymentMethod::Cash,
        PaymentMethod::Bank,
        PaymentMethod::Online,
        PaymentMethod::OneBill,
        PaymentMethod::Kuickpay,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PaymentMethod::Cash => "cash",
            PaymentMethod::Bank => "bank",
            PaymentMethod::Online => "online",
            PaymentMethod::OneBill => "one_bill",
            PaymentMethod::Kuickpay => "kuickpay",
        }
    }

    /// Methods that need a reference number.
    pub fn needs_reference(self) -> bool {
        !matches!(self, PaymentMethod::Cash)
    }
}

impl std::str::FromStr for PaymentMethod {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PaymentMethod::ALL
            .iter()
            .copied()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| format!("unknown payment method: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn money_rounds_half_away_from_zero_to_two_places() {
        assert_eq!(money(dec!(333.333)), dec!(333.33));
        assert_eq!(money(dec!(123.455)), dec!(123.46));
        assert_eq!(money(dec!(123.445)), dec!(123.45));
        assert_eq!(money(dec!(-0.005)), dec!(-0.01));
        assert_eq!(percent_of(dec!(3333.33), dec!(10)), dec!(333.33));
        assert_eq!(percent_of(dec!(1234.55), dec!(10)), dec!(123.46));
        assert_eq!(percent_of(dec!(2500), dec!(12.5)), dec!(312.50));
        // No floating point drift: 0.1 + 0.2 == 0.3.
        assert_eq!(money(dec!(0.1) + dec!(0.2)), dec!(0.3));
    }

    #[test]
    fn default_policy_is_ten_percent_after_the_fifteenth() {
        let p = FeePolicy::default();
        assert_eq!(p.sibling_discount_percent, dec!(10));
        assert_eq!(p.late_fee_percent, dec!(10));
        assert_eq!(p.due_day, 15);
        assert_eq!(p.sibling_rule, SiblingRule::All);
        assert!(p.validate().is_ok());
        assert_eq!(p.due_date(d(2026, 4, 1)), d(2026, 4, 15));
        let bad = FeePolicy {
            late_fee_percent: dec!(101),
            ..p.clone()
        };
        assert_eq!(bad.validate(), Err(PolicyError::Percent));
        let bad = FeePolicy { due_day: 31, ..p };
        assert_eq!(bad.validate(), Err(PolicyError::DueDay));
    }

    #[test]
    fn sibling_rules() {
        let all = FeePolicy::default();
        assert!(
            !all.sibling_discount_applies(0, 0),
            "only children get nothing"
        );
        assert!(all.sibling_discount_applies(1, 0));
        assert!(all.sibling_discount_applies(2, 2));
        let second = FeePolicy {
            sibling_rule: SiblingRule::SecondChildOnwards,
            ..FeePolicy::default()
        };
        assert!(
            !second.sibling_discount_applies(1, 0),
            "first child pays full"
        );
        assert!(second.sibling_discount_applies(1, 1));
        assert!(second.sibling_discount_applies(2, 2));
        let zero = FeePolicy {
            sibling_discount_percent: dec!(0),
            ..FeePolicy::default()
        };
        assert!(!zero.sibling_discount_applies(3, 1));
    }

    fn input(lines: &[(&str, Decimal)]) -> VoucherInput {
        VoucherInput {
            lines: lines.iter().map(|(h, a)| (h.to_string(), *a)).collect(),
            sibling_discount: false,
            concessions: vec![],
            arrears: Decimal::ZERO,
        }
    }

    #[test]
    fn sibling_discount_is_a_share_of_the_total_monthly_fee() {
        let policy = FeePolicy::default();
        let mut i = input(&[
            ("Tuition", dec!(2500)),
            ("Transport", dec!(800)),
            ("Exam", dec!(33.33)),
        ]);
        i.sibling_discount = true;
        let t = compute_voucher(&policy, &i);
        assert_eq!(t.gross, dec!(3333.33));
        assert_eq!(t.sibling_discount, dec!(333.33));
        assert_eq!(t.concession_discount, dec!(0));
        assert_eq!(t.net_monthly, dec!(3000.00));
        assert_eq!(t.total_due, dec!(3000.00));
        // Configurable percentage.
        let p15 = FeePolicy {
            sibling_discount_percent: dec!(15),
            ..FeePolicy::default()
        };
        assert_eq!(compute_voucher(&p15, &i).sibling_discount, dec!(500.00));
        // Not eligible → no discount.
        i.sibling_discount = false;
        assert_eq!(compute_voucher(&policy, &i).net_monthly, dec!(3333.33));
    }

    #[test]
    fn concessions_stack_but_never_exceed_gross() {
        let policy = FeePolicy::default();
        let mut i = input(&[("Tuition", dec!(2000))]);
        i.sibling_discount = true; // 200
        i.concessions = vec![
            ConcessionTerms {
                percent: Some(dec!(50)),
                amount: None,
            }, // 1000
            ConcessionTerms {
                percent: None,
                amount: Some(dec!(150.005)),
            }, // 150.01
        ];
        let t = compute_voucher(&policy, &i);
        assert_eq!(t.sibling_discount, dec!(200));
        assert_eq!(t.concession_discount, dec!(1150.01));
        assert_eq!(t.net_monthly, dec!(649.99));
        i.concessions = vec![ConcessionTerms {
            percent: Some(dec!(100)),
            amount: None,
        }];
        let t = compute_voucher(&policy, &i);
        assert_eq!(t.sibling_discount + t.concession_discount, dec!(2000));
        assert_eq!(t.net_monthly, dec!(0));
    }

    #[test]
    fn arrears_add_to_total_due_and_credits_reduce_it() {
        let policy = FeePolicy::default();
        let mut i = input(&[("Tuition", dec!(2500))]);
        i.arrears = dec!(1250.50);
        assert_eq!(compute_voucher(&policy, &i).total_due, dec!(3750.50));
        i.arrears = dec!(-100);
        assert_eq!(compute_voucher(&policy, &i).total_due, dec!(2400));
    }

    #[test]
    fn late_fee_applies_only_after_the_due_date_and_not_when_waived() {
        let policy = FeePolicy::default();
        let due = d(2026, 4, 15);
        assert_eq!(
            late_fee(&policy, dec!(3000), due, d(2026, 4, 15), false),
            dec!(0),
            "on the due date"
        );
        assert_eq!(
            late_fee(&policy, dec!(3000), due, d(2026, 4, 16), false),
            dec!(300),
            "the day after"
        );
        assert_eq!(
            late_fee(&policy, dec!(3000), due, d(2026, 6, 1), false),
            dec!(300),
            "flat, not per day"
        );
        assert_eq!(
            late_fee(&policy, dec!(3000), due, d(2026, 4, 16), true),
            dec!(0),
            "waived"
        );
        assert_eq!(
            late_fee(&policy, dec!(2666.67), due, d(2026, 4, 20), false),
            dec!(266.67)
        );
        let p5 = FeePolicy {
            late_fee_percent: dec!(5),
            due_day: 10,
            ..FeePolicy::default()
        };
        let due = p5.due_date(d(2026, 4, 1));
        assert_eq!(due, d(2026, 4, 10));
        assert_eq!(
            late_fee(&p5, dec!(3000), due, d(2026, 4, 11), false),
            dec!(150)
        );
    }

    #[test]
    fn balance_status_and_partial_payments() {
        assert_eq!(
            voucher_balance(dec!(3000), dec!(300), dec!(1000)),
            dec!(2300)
        );
        assert_eq!(
            VoucherStatus::derive(dec!(2300), dec!(1000)),
            VoucherStatus::Partial
        );
        assert_eq!(
            VoucherStatus::derive(dec!(3300), dec!(0)),
            VoucherStatus::Unpaid
        );
        assert_eq!(
            VoucherStatus::derive(dec!(0), dec!(3300)),
            VoucherStatus::Paid
        );
        assert_eq!(
            VoucherStatus::derive(dec!(-50), dec!(3350)),
            VoucherStatus::Paid,
            "overpaid counts as paid"
        );
    }

    #[test]
    fn reversal_restores_the_balance_exactly() {
        // Payment 1,000 then reversed (−1,000): net paid 0, balance back to total due.
        let net_paid = dec!(1000) + dec!(-1000);
        assert_eq!(voucher_balance(dec!(3000), dec!(0), net_paid), dec!(3000));
        assert_eq!(
            VoucherStatus::derive(dec!(3000), net_paid),
            VoucherStatus::Unpaid
        );
        // Partial reversal of a fully paid voucher.
        let net_paid = dec!(3000) + dec!(-500.25);
        assert_eq!(voucher_balance(dec!(3000), dec!(0), net_paid), dec!(500.25));
    }

    #[test]
    fn carry_forward_across_months_and_years() {
        let policy = FeePolicy::default();
        // April: 2,500 billed, 1,000 paid late → late fee 250 → balance 1,750.
        let april = compute_voucher(&policy, &input(&[("Tuition", dec!(2500))]));
        let april_late = late_fee(
            &policy,
            april.net_monthly,
            d(2026, 4, 15),
            d(2026, 4, 20),
            false,
        );
        let april_balance = voucher_balance(april.total_due, april_late, dec!(1000));
        assert_eq!(april_balance, dec!(1750));
        // May carries April's balance as arrears.
        let mut may = input(&[("Tuition", dec!(2500))]);
        may.arrears = carry_forward(&[april_balance]);
        let may_t = compute_voucher(&policy, &may);
        assert_eq!(may_t.arrears, dec!(1750));
        assert_eq!(may_t.total_due, dec!(4250));
        // Nothing paid in May; the next academic year's first voucher carries both balances.
        let may_balance = voucher_balance(may_t.total_due, dec!(0), dec!(0));
        // The ledger counts each month's own charges once, so the balance is not double counted.
        let ledger = ledger_totals(&[
            LedgerEntry {
                gross: april.gross,
                discounts: dec!(0),
                late_fee: april_late,
                net_paid: dec!(1000),
            },
            LedgerEntry {
                gross: may_t.gross,
                discounts: dec!(0),
                late_fee: dec!(0),
                net_paid: dec!(0),
            },
        ]);
        assert_eq!(ledger.billed, dec!(5000));
        assert_eq!(ledger.fines, dec!(250));
        assert_eq!(ledger.paid, dec!(1000));
        assert_eq!(ledger.balance, dec!(4250));
        // Arrears for the first voucher of next year = the outstanding voucher balances, which
        // equals the ledger balance (April's unpaid 1,750 is already inside May's 4,250).
        let next_year_arrears = carry_forward(&[may_balance]);
        assert_eq!(next_year_arrears, ledger.balance);
        // Overpayment carries as credit.
        assert_eq!(carry_forward(&[dec!(-200), dec!(50)]), dec!(-150));
    }

    #[test]
    fn month_helpers() {
        assert_eq!(add_months(d(2026, 11, 1), 2), d(2027, 1, 1));
        assert_eq!(add_months(d(2026, 1, 1), -1), d(2025, 12, 1));
        let months = billing_months(d(2026, 4, 1), d(2027, 3, 31));
        assert_eq!(months.len(), 12);
        assert_eq!(months[0], d(2026, 4, 1));
        assert_eq!(months[11], d(2027, 3, 1));
    }

    #[test]
    fn payment_methods_round_trip() {
        for m in PaymentMethod::ALL {
            assert_eq!(m.as_str().parse::<PaymentMethod>().unwrap(), *m);
            assert_eq!(
                serde_json::to_string(m).unwrap(),
                format!("\"{}\"", m.as_str())
            );
        }
        assert!(PaymentMethod::Kuickpay.needs_reference());
        assert!(!PaymentMethod::Cash.needs_reference());
    }
}
