//! Automatic late fees. A daily pass (and one at app start) adds the policy's late fee to every
//! unpaid voucher past its due date, so dashboards, defaulter lists and ledgers always show the
//! amount actually owed. Waived vouchers are never re-assessed, paid and carried vouchers are
//! left alone, and each fee is audited as `late_fee` (by the system actor when automatic, by the
//! user when run by hand from `POST /fees/late-fees/run`).

use chrono::NaiveDate;
use madrasa_db::fees::{self, LateFeeReport};
use madrasa_db::{schools, Actor};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::{ApiError, AppState};

/// Assess the late fees due as of `as_of` for one school, in one transaction.
pub async fn run(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    as_of: NaiveDate,
) -> Result<LateFeeReport, ApiError> {
    let mut tx = state.pool.begin().await?;
    let report = fees::assess_due_late_fees(&mut tx, &state.hlc, actor, school_id, as_of).await?;
    tx.commit().await?;
    Ok(report)
}

/// Is the daily pass due: it has not run yet, or last ran on an earlier (local) day.
pub fn should_run(last_run: Option<NaiveDate>, today: NaiveDate) -> bool {
    last_run.is_none_or(|d| d < today)
}

/// How long a failed pass waits before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// Background task: runs the pass once at start and again after every local midnight. The date
/// is checked once a minute, so a PC that slept through midnight catches up when it wakes. A
/// failed pass (database unavailable) is retried after ten minutes.
pub fn spawn_scheduler(state: AppState) {
    tokio::spawn(async move {
        let mut last_run: Option<NaiveDate> = None;
        let mut last_failure: Option<Instant> = None;
        loop {
            let today = crate::clock::today();
            let backing_off = last_failure.is_some_and(|f| f.elapsed() < RETRY_AFTER);
            if should_run(last_run, today) && !backing_off {
                match tick(&state, today).await {
                    Ok(Some(report)) => {
                        if report.assessed > 0 {
                            tracing::info!(
                                assessed = report.assessed,
                                total = %report.total,
                                "late fees added to overdue vouchers"
                            );
                        }
                        last_run = Some(today);
                        last_failure = None;
                    }
                    // No school yet (setup not done): try again next minute.
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "automatic late fee pass failed");
                        last_failure = Some(Instant::now());
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tick(state: &AppState, today: NaiveDate) -> anyhow::Result<Option<LateFeeReport>> {
    let mut c = state.pool.acquire().await?;
    let Some(school) = schools::current(&mut c).await? else {
        return Ok(None);
    };
    drop(c);
    let report = run(state, Actor::system_for(school.id), school.id, today).await?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn runs_at_start_and_once_per_day() {
        assert!(should_run(None, d(2026, 4, 16)), "never run: at app start");
        assert!(
            !should_run(Some(d(2026, 4, 16)), d(2026, 4, 16)),
            "already done today"
        );
        assert!(
            should_run(Some(d(2026, 4, 16)), d(2026, 4, 17)),
            "after midnight"
        );
        assert!(
            should_run(Some(d(2026, 4, 16)), d(2026, 4, 20)),
            "PC was off for days: catch up once"
        );
        assert!(
            !should_run(Some(d(2026, 4, 17)), d(2026, 4, 16)),
            "clock set back: do not run twice"
        );
    }
}
