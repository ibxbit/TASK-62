/// Background job scheduler.
///
/// ## Design
///
/// Each registered job runs in its own dedicated tokio task on a fixed interval.
/// A shared `ShutdownFlag` lets `main()` ask all loops to exit cleanly after
/// their current sleep completes — no in-flight work is ever interrupted.
///
/// ## Execution guarantees
///
/// **At-least-once with strong deduplication** via PostgreSQL advisory locks:
///
/// | Scenario                                            | Outcome                    |
/// |-----------------------------------------------------|----------------------------|
/// | Normal operation                                    | Exactly-once per interval  |
/// | Second process starts (rolling deploy)              | Skip — lock held elsewhere |
/// | Process crashes mid-job                             | PG releases lock on session close; next instance retries |
/// | `job_runs` INSERT fails (transient DB error)        | Job still runs; logged as "unrecorded" |
/// | `job_runs` UPDATE fails after successful run        | Status stays `running`; repaired on restart by `recover_stale_runs` |
///
/// Advisory lock IDs are derived from the job name with FNV-1a so they are
/// stable across restarts without requiring a lock table.
///
/// ## Idempotency requirement
///
/// Because the guarantee is at-least-once, **every job must be idempotent**.
/// The individual jobs achieve this via:
/// - `FOR UPDATE SKIP LOCKED` on the rows they process
/// - Status guards (`WHERE status = 'pending'`, `WHERE processed_at IS NULL`)
/// - Upserts with `ON CONFLICT DO NOTHING / DO UPDATE`
pub mod executor;
pub mod jobs;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use sqlx::PgPool;

// ── Job outcome / error ───────────────────────────────────────────────────────

/// Returned by a successful `Job::run` invocation.
/// Stored in `scheduler.job_runs.outcome` as JSONB.
#[derive(Debug)]
pub struct JobOutcome {
    pub summary: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Failed(String),
}

// ── Job trait ─────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Job: Send + Sync + 'static {
    /// Stable unique name stored in `scheduler.job_runs.job_name`.
    fn name(&self) -> &'static str;

    /// How often the job should run.
    fn interval(&self) -> Duration;

    /// Maximum elapsed time a `running` row can show before it is considered a
    /// stale record from a crashed process.  Defaults to 2 × interval.
    fn lock_timeout(&self) -> Duration {
        self.interval() * 2
    }

    /// Execute the job's work.  Must be idempotent.
    async fn run(&self, pool: &PgPool) -> Result<JobOutcome, JobError>;
}

// ── Shutdown flag ─────────────────────────────────────────────────────────────

/// A shareable boolean that signals all job loops to stop on their next
/// sleep boundary.  In-progress work is never interrupted.
#[derive(Clone)]
pub struct ShutdownFlag(Arc<AtomicBool>);

impl ShutdownFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Signal all job loops to exit after their current sleep.
    pub fn signal(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

// ── Scheduler ─────────────────────────────────────────────────────────────────

pub struct Scheduler {
    pool:     PgPool,
    shutdown: ShutdownFlag,
}

impl Scheduler {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            shutdown: ShutdownFlag::new(),
        }
    }

    /// Returns a clone of the shutdown flag so `main()` can trigger a graceful stop.
    pub fn shutdown_flag(&self) -> ShutdownFlag {
        self.shutdown.clone()
    }

    /// Spawn a job as a background tokio task.
    /// The task runs until the shutdown flag is set.
    pub fn spawn<J: Job>(&self, job: J) {
        let pool     = self.pool.clone();
        let shutdown = self.shutdown.clone();
        let job      = Arc::new(job);

        tracing::info!(
            job      = job.name(),
            interval = ?job.interval(),
            "Registering job"
        );

        tokio::spawn(async move {
            executor::job_loop(pool, job, shutdown).await;
        });
    }

    /// Repair stale `running` records left by a previously-crashed process.
    /// Call once during startup before spawning job loops.
    pub async fn recover_stale_runs(&self) -> Result<(), sqlx::Error> {
        let n = sqlx::query(
            r#"
            UPDATE scheduler.job_runs
            SET    status      = 'failed',
                   error_msg   = 'Recovered: process crashed before job completed',
                   finished_at = now()
            WHERE  status      = 'running'
              AND  started_at  < now() - interval '1 hour'
            "#,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        if n > 0 {
            tracing::warn!(count = n, "Recovered stale running job_runs records");
        }
        Ok(())
    }
}
