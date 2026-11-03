//! Admit work before spawning; hold its permit until the blocking job actually exits.
pub(crate) use glim_services::work::scheduler::{JobClass, Priority};
use glim_services::work::{Cancellation, scheduler::Scheduler};
use gpui_kit::BackgroundExecutor;

pub(crate) async fn run<T: Send + 'static>(
    executor: BackgroundExecutor,
    class: JobClass,
    priority: Priority,
    cancel: Cancellation,
    work: impl FnOnce(&Cancellation) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let queued = std::time::Instant::now();
    let permit = Scheduler::global()
        .acquire(class, priority, cancel.clone())
        .await?;
    cancel.check()?;
    let queue_ms = queued.elapsed().as_millis() as u64;
    executor
        .spawn(async move {
            let _permit = permit;
            cancel.check()?;
            let started = std::time::Instant::now();
            let result = work(&cancel);
            use glim_services::telemetry::{number, text, track};
            track(
                "reader_job",
                [
                    ("class", text(class.name())),
                    ("priority", text(priority.name())),
                    ("queue_ms", number(queue_ms)),
                    ("work_ms", number(started.elapsed().as_millis() as u64)),
                ],
            );
            cancel.check()?;
            result
        })
        .await
}

/// Start bounded lookahead work without waiting for its consumer to poll it.
pub(crate) fn spawn<T: Send + 'static>(
    executor: BackgroundExecutor,
    class: JobClass,
    priority: Priority,
    cancel: Cancellation,
    work: impl FnOnce(&Cancellation) -> anyhow::Result<T> + Send + 'static,
) -> gpui_kit::Task<anyhow::Result<T>> {
    let worker = executor.clone();
    executor.spawn(async move { run(worker, class, priority, cancel, work).await })
}
