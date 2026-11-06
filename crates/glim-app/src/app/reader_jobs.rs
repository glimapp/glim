//! Admit work before spawning; hold its permit until the blocking job actually exits.
pub(crate) use glim_services::work::scheduler::{JobClass, Priority};
use glim_services::work::{Cancellation, scheduler::Scheduler};
use gpui_kit::BackgroundExecutor;
use std::sync::Arc;

pub(crate) async fn run<T: Send + 'static>(
    executor: BackgroundExecutor,
    class: JobClass,
    priority: Priority,
    cancel: Cancellation,
    work: impl FnOnce(&Cancellation) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    run_with_scheduler(
        executor,
        Scheduler::global().clone(),
        class,
        priority,
        cancel,
        work,
    )
    .await
}

pub(crate) async fn run_with_scheduler<T: Send + 'static>(
    executor: BackgroundExecutor,
    scheduler: Arc<Scheduler>,
    class: JobClass,
    priority: Priority,
    cancel: Cancellation,
    work: impl FnOnce(&Cancellation) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let permit = scheduler.acquire(class, priority, cancel.clone()).await?;
    cancel.check()?;
    executor
        .spawn(async move {
            let _permit = permit;
            cancel.check()?;
            let result = work(&cancel);
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
