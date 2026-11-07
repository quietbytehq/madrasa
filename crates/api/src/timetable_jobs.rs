//! Timetable generation runs (Standard stage 1). A run solves in a blocking thread with a
//! time limit; its progress is kept here so the screen can poll it, and the result is
//! written to the timetable's cards when it finishes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use madrasa_core::timetable::solver::{Phase, Progress};
use madrasa_core::timetable::{self as engine, Options, SolveReport};
use madrasa_db::timetable as repo;
use madrasa_db::{Actor, PgPool};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// A run as the screen sees it. Mirrored by `TimetableJob` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub id: Uuid,
    pub timetable_id: Uuid,
    pub academic_year_id: Uuid,
    /// `running`, `done` or `failed`.
    pub status: String,
    /// `placing`, `repairing`, `improving` or `done`.
    pub phase: String,
    pub placed: usize,
    pub total: usize,
    pub fraction: f64,
    pub error: Option<String>,
    /// The generator's report once done (placed, unplaced with reasons, quality).
    pub report: Option<Value>,
}

struct Job {
    school_id: Uuid,
    view: JobView,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct TimetableJobs {
    jobs: Mutex<HashMap<Uuid, Job>>,
}

/// What a run needs: the setup, the locked cards and the options.
pub struct RunInput {
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub timetable_id: Uuid,
    pub setup: engine::SchoolSetup,
    pub locked: Vec<engine::PlacedCard>,
    pub time_limit: Duration,
    pub improve: Duration,
    pub seed: u64,
}

fn phase_name(p: &Phase) -> &'static str {
    match p {
        Phase::Placing => "placing",
        Phase::Improving => "improving",
    }
}

/// The report without its (large) card list; the cards are stored as rows.
pub fn report_json(r: &SolveReport) -> Value {
    let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.remove("timetable");
    }
    v
}

impl TimetableJobs {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, school_id: Uuid, id: Uuid) -> Option<JobView> {
        self.lock()
            .get(&id)
            .filter(|j| j.school_id == school_id)
            .map(|j| j.view.clone())
    }

    /// The run in progress for a timetable, if any.
    pub fn running_for(&self, timetable_id: Uuid) -> Option<JobView> {
        self.lock()
            .values()
            .find(|j| j.view.timetable_id == timetable_id && j.view.status == "running")
            .map(|j| j.view.clone())
    }

    pub fn cancel(&self, school_id: Uuid, id: Uuid) -> bool {
        match self.lock().get(&id) {
            Some(j) if j.school_id == school_id => {
                j.cancel.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn update(&self, id: Uuid, f: impl FnOnce(&mut JobView)) {
        if let Some(j) = self.lock().get_mut(&id) {
            f(&mut j.view);
        }
    }

    /// Start a run. Returns its id at once; poll with [`TimetableJobs::get`].
    pub fn start(
        self: &Arc<Self>,
        pool: PgPool,
        hlc: Arc<madrasa_core::hlc::Hlc>,
        actor: Actor,
        input: RunInput,
    ) -> Uuid {
        let id = madrasa_core::ids::new_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let total = engine::lessons::generate_cards(&input.setup.lessons).len();
        {
            let mut jobs = self.lock();
            // Forget finished runs older than the newest few so the map stays small.
            let finished: Vec<Uuid> = jobs
                .iter()
                .filter(|(_, j)| j.view.status != "running")
                .map(|(k, _)| *k)
                .collect();
            if finished.len() > 20 {
                for k in finished {
                    jobs.remove(&k);
                }
            }
            jobs.insert(
                id,
                Job {
                    school_id: input.school_id,
                    cancel: cancel.clone(),
                    view: JobView {
                        id,
                        timetable_id: input.timetable_id,
                        academic_year_id: input.academic_year_id,
                        status: "running".into(),
                        phase: "placing".into(),
                        placed: 0,
                        total,
                        fraction: 0.0,
                        error: None,
                        report: None,
                    },
                },
            );
        }
        let jobs = self.clone();
        tokio::spawn(async move {
            let school_id = input.school_id;
            let timetable_id = input.timetable_id;
            let setup = input.setup;
            let locked = input.locked;
            let opts = Options {
                time_limit: input.time_limit,
                improve_time: input.improve,
                seed: input.seed,
                ..Default::default()
            };
            let progress_jobs = jobs.clone();
            let cancel_flag = cancel.clone();
            let solved = tokio::task::spawn_blocking(move || {
                let mut on_progress = |p: Progress| {
                    progress_jobs.update(id, |v| {
                        v.phase = phase_name(&p.phase).into();
                        v.placed = p.placed;
                        v.total = p.total;
                        v.fraction = p.fraction;
                    });
                };
                engine::solve(&setup, &locked, &opts, &cancel_flag, &mut on_progress)
            })
            .await;
            let report = match solved {
                Ok(r) => r,
                Err(_) => {
                    jobs.update(id, |v| {
                        v.status = "failed".into();
                        v.error = Some("The generator stopped unexpectedly.".into());
                    });
                    return;
                }
            };
            let placed: Vec<repo::PlacedInput> = report
                .timetable
                .cards
                .iter()
                .filter_map(repo::PlacedInput::from_engine)
                .collect();
            let json = report_json(&report);
            let saved = async {
                let mut tx = pool.begin().await?;
                repo::replace_cards(
                    &mut tx,
                    &hlc,
                    actor,
                    school_id,
                    timetable_id,
                    &placed,
                    Some(json.clone()),
                )
                .await?;
                tx.commit().await?;
                Ok::<(), madrasa_db::DbError>(())
            }
            .await;
            jobs.update(id, |v| match saved {
                Ok(()) => {
                    v.status = "done".into();
                    v.phase = "done".into();
                    v.placed = report.placed as usize;
                    v.total = report.total as usize;
                    v.fraction = 1.0;
                    v.report = Some(json);
                }
                Err(e) => {
                    v.status = "failed".into();
                    v.error = Some(format!("The timetable could not be saved: {e}"));
                }
            });
        });
        id
    }
}
