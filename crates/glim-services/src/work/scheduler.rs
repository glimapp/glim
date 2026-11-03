//! Process-wide admission for reader preparation. Waiting never occupies a worker.
use super::Cancellation;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll, Waker},
};

#[derive(Clone, Copy)]
pub enum JobClass {
    Document,
    Projection,
    Decode,
    Native,
    Diagram,
}
impl JobClass {
    fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Projection => "projection",
            Self::Decode => "decode",
            Self::Native => "native",
            Self::Diagram => "diagram",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Viewport,
    Interactive,
    Background,
}
impl Priority {
    pub fn name(self) -> &'static str {
        match self {
            Self::Viewport => "viewport",
            Self::Interactive => "interactive",
            Self::Background => "background",
        }
    }
}
struct Waiting {
    id: u64,
    priority: Priority,
    waker: Waker,
}
struct Lane {
    active: usize,
    limit: usize,
    next: usize,
    waiting: VecDeque<Waiting>,
}
// Visible work gets two turns per cycle; clicks and background work retain progress.
const PRIORITIES: [Priority; 4] = [
    Priority::Viewport,
    Priority::Interactive,
    Priority::Viewport,
    Priority::Background,
];
impl Lane {
    fn selected(&self) -> Option<(usize, usize)> {
        if self.active >= self.limit {
            return None;
        }
        (0..PRIORITIES.len()).find_map(|offset| {
            let turn = (self.next + offset) % PRIORITIES.len();
            self.waiting
                .iter()
                .position(|job| job.priority == PRIORITIES[turn])
                .map(|index| (index, turn))
        })
    }
    fn wake_next(&self) -> Option<Waker> {
        self.selected()
            .map(|(index, _)| self.waiting[index].waker.clone())
    }
}

struct State {
    lanes: [Lane; 5],
    next_id: u64,
}
pub struct Scheduler {
    state: Mutex<State>,
}
impl Scheduler {
    fn new(limits: [usize; 5]) -> Arc<Self> {
        assert!(limits.iter().all(|limit| *limit > 0));
        Arc::new(Self {
            state: Mutex::new(State {
                lanes: limits.map(|limit| Lane {
                    limit,
                    active: 0,
                    next: 0,
                    waiting: VecDeque::new(),
                }),
                next_id: 0,
            }),
        })
    }
    pub fn global() -> &'static Arc<Self> {
        static SCHEDULER: OnceLock<Arc<Scheduler>> = OnceLock::new();
        SCHEDULER.get_or_init(|| {
            Self::new([
                crate::limits::JOBS_DOCUMENT_CONCURRENCY,
                crate::limits::JOBS_PROJECTION_CONCURRENCY,
                crate::limits::JOBS_DECODE_CONCURRENCY,
                crate::limits::JOBS_NATIVE_CONCURRENCY,
                crate::limits::JOBS_DIAGRAM_CONCURRENCY,
            ])
        })
    }
    pub fn acquire(
        self: &Arc<Self>,
        class: JobClass,
        priority: Priority,
        cancel: Cancellation,
    ) -> Admission {
        Admission {
            scheduler: self.clone(),
            class,
            priority,
            cancel,
            id: None,
        }
    }
}
pub struct Admission {
    scheduler: Arc<Scheduler>,
    class: JobClass,
    priority: Priority,
    cancel: Cancellation,
    id: Option<u64>,
}
pub struct Permit {
    scheduler: Arc<Scheduler>,
    class: JobClass,
}
impl Future for Admission {
    type Output = anyhow::Result<Permit>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(error) = self.cancel.check() {
            self.remove();
            return Poll::Ready(Err(error));
        }
        let scheduler = self.scheduler.clone();
        let mut state = scheduler
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let id = match self.id {
            Some(id) => id,
            None => {
                let id = state.next_id;
                state.next_id = state.next_id.wrapping_add(1);
                state.lanes[self.class.index()].waiting.push_back(Waiting {
                    id,
                    priority: self.priority,
                    waker: cx.waker().clone(),
                });
                self.id = Some(id);
                id
            }
        };
        let lane = &mut state.lanes[self.class.index()];
        let index = lane
            .waiting
            .iter()
            .position(|job| job.id == id)
            .expect("registered admission");
        lane.waiting[index].waker.clone_from(cx.waker());
        let Some((selected, turn)) = lane.selected() else {
            return Poll::Pending;
        };
        if selected != index {
            return Poll::Pending;
        }
        lane.waiting.remove(index);
        lane.active += 1;
        lane.next = (turn + 1) % PRIORITIES.len();
        self.id = None;
        let wake = lane.wake_next();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
        Poll::Ready(Ok(Permit {
            scheduler,
            class: self.class,
        }))
    }
}
impl Admission {
    fn remove(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let mut state = self
            .scheduler
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let lane = &mut state.lanes[self.class.index()];
        lane.waiting.retain(|job| job.id != id);
        let wake = lane.wake_next();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.remove();
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self
            .scheduler
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let lane = &mut state.lanes[self.class.index()];
        lane.active -= 1;
        let wake = lane.wake_next();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn poll(admission: &mut Admission) -> Poll<anyhow::Result<Permit>> {
        Pin::new(admission).poll(&mut Context::from_waker(Waker::noop()))
    }
    fn ready(admission: &mut Admission) -> Permit {
        match poll(admission) {
            Poll::Ready(Ok(permit)) => permit,
            _ => panic!("expected admission"),
        }
    }
    fn request(pool: &Arc<Scheduler>, class: JobClass, priority: Priority) -> Admission {
        pool.acquire(class, priority, Cancellation::default())
    }
    #[test]
    fn releasing_capacity_wakes_a_waiter() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counter(AtomicUsize);
        impl std::task::Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let pool = Scheduler::new([1; 5]);
        let first = ready(&mut request(
            &pool,
            JobClass::Document,
            Priority::Interactive,
        ));
        let mut next = request(&pool, JobClass::Document, Priority::Interactive);
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        assert!(
            Pin::new(&mut next)
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
        drop(first);
        assert!(counter.0.load(Ordering::Relaxed) > 0);
        drop(ready(&mut next));
    }

    #[test]
    fn running_work_holds_capacity_after_its_caller_is_canceled() {
        let pool = Scheduler::new([1; 5]);
        let (cancel, owner) = Cancellation::task();
        let first = ready(&mut pool.acquire(JobClass::Decode, Priority::Interactive, cancel));
        let mut next = request(&pool, JobClass::Decode, Priority::Interactive);
        assert!(poll(&mut next).is_pending());
        drop(owner);
        assert!(poll(&mut next).is_pending());
        let other_lane = ready(&mut request(
            &pool,
            JobClass::Document,
            Priority::Interactive,
        ));
        drop(other_lane);
        drop(first);
        drop(ready(&mut next));
    }
    #[test]
    fn dropped_and_canceled_waiters_do_not_block_the_queue() {
        let pool = Scheduler::new([1; 5]);
        let first = ready(&mut request(
            &pool,
            JobClass::Projection,
            Priority::Interactive,
        ));
        let mut abandoned = request(&pool, JobClass::Projection, Priority::Background);
        assert!(poll(&mut abandoned).is_pending());
        let (cancel, owner) = Cancellation::task();
        let mut canceled = pool.acquire(JobClass::Projection, Priority::Background, cancel);
        assert!(poll(&mut canceled).is_pending());
        drop(abandoned);
        drop(owner);
        assert!(
            matches!(poll(&mut canceled), Poll::Ready(Err(error)) if error.is::<super::super::Cancelled>())
        );
        drop(first);
        drop(ready(&mut request(
            &pool,
            JobClass::Projection,
            Priority::Interactive,
        )));
    }
    #[test]
    fn both_priorities_make_progress_without_reordering_their_own_queue() {
        let pool = Scheduler::new([1; 5]);
        let first = ready(&mut request(
            &pool,
            JobClass::Document,
            Priority::Interactive,
        ));
        let mut background = request(&pool, JobClass::Document, Priority::Background);
        let mut first_click = request(&pool, JobClass::Document, Priority::Interactive);
        let mut second_click = request(&pool, JobClass::Document, Priority::Interactive);
        assert!(poll(&mut background).is_pending());
        assert!(poll(&mut first_click).is_pending());
        assert!(poll(&mut second_click).is_pending());
        drop(first);
        assert!(poll(&mut second_click).is_pending());
        assert!(poll(&mut first_click).is_pending());
        drop(ready(&mut background));
        assert!(poll(&mut second_click).is_pending());
        drop(ready(&mut first_click));
        drop(ready(&mut second_click));
    }
    #[test]
    fn viewport_has_two_turns_without_starving_clicks_or_background() {
        let pool = Scheduler::new([1; 5]);
        let first = ready(&mut request(&pool, JobClass::Decode, Priority::Background));
        let mut visible1 = request(&pool, JobClass::Decode, Priority::Viewport);
        let mut visible2 = request(&pool, JobClass::Decode, Priority::Viewport);
        let mut visible3 = request(&pool, JobClass::Decode, Priority::Viewport);
        let mut click = request(&pool, JobClass::Decode, Priority::Interactive);
        let mut background = request(&pool, JobClass::Decode, Priority::Background);
        for job in [
            &mut visible1,
            &mut visible2,
            &mut visible3,
            &mut click,
            &mut background,
        ] {
            assert!(poll(job).is_pending());
        }
        drop(first);
        assert!(poll(&mut visible2).is_pending());
        drop(ready(&mut visible1));
        assert!(poll(&mut visible2).is_pending());
        drop(ready(&mut click));
        drop(ready(&mut visible2));
        assert!(poll(&mut visible3).is_pending());
        drop(ready(&mut background));
        drop(ready(&mut visible3));
    }

    #[test]
    fn unwinding_a_worker_returns_its_permit() {
        let pool = Scheduler::new([1; 5]);
        let permit = ready(&mut request(&pool, JobClass::Native, Priority::Interactive));
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _permit = permit;
            panic!("reader failed");
        }));
        assert!(failure.is_err());
        drop(ready(&mut request(
            &pool,
            JobClass::Native,
            Priority::Interactive,
        )));
    }
}
