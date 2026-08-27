//! Deterministic coordination for lazy, application-specific component work.
//!
//! The table is an optimization only. One caller owns a missing key while
//! concurrent top-level callers wait for that exact attempt. Recursive callers
//! never wait for running work: they receive [`ComponentWorkOutcome::RecursiveDependency`]
//! and must use the ordinary canonical port search instead. This conservative
//! rule also prevents cross-thread dependency cycles without making component
//! availability part of completeness.
//!
//! Only [`ComponentWorkCompletion::Complete`] enters the completed cache.
//! Incomplete and abandoned attempts wake their current waiters, leave no cache
//! entry, and therefore cannot become persistable through this table.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

thread_local! {
    static ACTIVE_TABLES: RefCell<Vec<Arc<TableIdentity>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
struct TableIdentity;

/// Read/write port for proof-complete component records.
///
/// Implementations may use `SQLite`, memory, or a disabled store. Persistent
/// implementations must validate and quarantine records before returning them
/// from [`Self::load_complete`].
pub trait ComponentRepository<K, V>: Send + Sync {
    /// Repository-specific failure.
    type Error;

    /// Loads a proof-complete value for `key` when one is available.
    ///
    /// # Errors
    ///
    /// Returns the repository's error if the lookup cannot be completed.
    fn load_complete(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Stores one proof-complete value transactionally.
    ///
    /// Callers must invoke this only after observing
    /// [`ComponentWorkOutcome::Complete`].
    ///
    /// # Errors
    ///
    /// Returns the repository's error if the transaction does not commit.
    fn store_complete(&self, key: &K, value: &V) -> Result<(), Self::Error>;
}

/// Read-only port used by search code to obtain optional component records.
///
/// The value may be one component or a deterministically ordered collection of
/// components. Keeping the port generic lets solver-core remain independent of
/// any persistence implementation.
pub trait ComponentProvider<K, V>: Send + Sync {
    /// Provider-specific failure.
    type Error;

    /// Returns a component value for `key`, if this provider has one.
    ///
    /// # Errors
    ///
    /// Returns the provider's error if the lookup cannot be completed.
    fn provide(&self, key: &K) -> Result<Option<V>, Self::Error>;
}

/// Result produced by the owner of one work-table key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentWorkCompletion<V, I> {
    /// The proof obligation completed and the value may be cached.
    Complete(V),
    /// The proof obligation stopped early and the value must not be cached.
    Incomplete(I),
}

/// Result observed by a work-table caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentWorkOutcome<V, I> {
    /// Proof-complete cached or newly computed value.
    Complete(Arc<V>),
    /// The shared attempt stopped without completing its proof obligation.
    Incomplete(Arc<I>),
    /// A recursive caller encountered running work and must use primitive
    /// canonical search for this dependency.
    RecursiveDependency,
    /// The owner unwound or dropped its attempt before publishing a result.
    Abandoned,
}

impl<V, I> ComponentWorkOutcome<V, I> {
    /// Borrows the value only when this outcome is proof-complete.
    #[must_use]
    pub fn complete_value(&self) -> Option<&V> {
        match self {
            Self::Complete(value) => Some(value),
            Self::Incomplete(_) | Self::RecursiveDependency | Self::Abandoned => None,
        }
    }
}

/// Public state of one key in a [`ComponentWorkTable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentWorkState {
    /// One caller currently owns the proof obligation.
    InProgress,
    /// The table holds a proof-complete value.
    Complete,
}

#[derive(Debug)]
enum Entry<V, I> {
    InProgress(Arc<RunningWork<V, I>>),
    Complete(Arc<V>),
}

#[derive(Debug)]
struct RunningWork<V, I> {
    outcome: Mutex<Option<PublishedOutcome<V, I>>>,
    ready: Condvar,
}

impl<V, I> RunningWork<V, I> {
    fn new() -> Self {
        Self {
            outcome: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn publish(&self, outcome: PublishedOutcome<V, I>) {
        let mut published = lock_unpoisoned(&self.outcome);
        if published.is_none() {
            *published = Some(outcome);
        }
        drop(published);
        self.ready.notify_all();
    }

    fn wait(&self) -> ComponentWorkOutcome<V, I> {
        let mut published = lock_unpoisoned(&self.outcome);
        loop {
            match &*published {
                Some(PublishedOutcome::Complete(value)) => {
                    return ComponentWorkOutcome::Complete(Arc::clone(value));
                }
                Some(PublishedOutcome::Incomplete(reason)) => {
                    return ComponentWorkOutcome::Incomplete(Arc::clone(reason));
                }
                Some(PublishedOutcome::Abandoned) => {
                    return ComponentWorkOutcome::Abandoned;
                }
                None => {
                    published = self
                        .ready
                        .wait(published)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
        }
    }
}

#[derive(Debug)]
enum PublishedOutcome<V, I> {
    Complete(Arc<V>),
    Incomplete(Arc<I>),
    Abandoned,
}

/// Deterministic same-key work deduplication for lazy component optimization.
///
/// Keys live in a [`BTreeMap`], so snapshots and completed-cache traversal do
/// not depend on insertion order or thread scheduling. The table performs no
/// persistence itself; callers may write a value through
/// [`ComponentRepository::store_complete`] only after matching a
/// [`ComponentWorkOutcome::Complete`].
#[derive(Debug)]
pub struct ComponentWorkTable<K, V, I> {
    identity: Arc<TableIdentity>,
    entries: Mutex<BTreeMap<K, Entry<V, I>>>,
}

impl<K, V, I> Default for ComponentWorkTable<K, V, I> {
    fn default() -> Self {
        Self {
            identity: Arc::new(TableIdentity),
            entries: Mutex::new(BTreeMap::new()),
        }
    }
}

impl<K, V, I> ComponentWorkTable<K, V, I>
where
    K: Clone + Ord,
{
    /// Creates an empty work table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current state for `key`.
    #[must_use]
    pub fn state(&self, key: &K) -> Option<ComponentWorkState> {
        lock_unpoisoned(&self.entries)
            .get(key)
            .map(|entry| match entry {
                Entry::InProgress(_) => ComponentWorkState::InProgress,
                Entry::Complete(_) => ComponentWorkState::Complete,
            })
    }

    /// Returns every current state in ascending key order.
    #[must_use]
    pub fn ordered_states(&self) -> Vec<(K, ComponentWorkState)> {
        lock_unpoisoned(&self.entries)
            .iter()
            .map(|(key, entry)| {
                let state = match entry {
                    Entry::InProgress(_) => ComponentWorkState::InProgress,
                    Entry::Complete(_) => ComponentWorkState::Complete,
                };
                (key.clone(), state)
            })
            .collect()
    }

    /// Returns a completed cached value without starting work.
    #[must_use]
    pub fn cached_complete(&self, key: &K) -> Option<Arc<V>> {
        let entries = lock_unpoisoned(&self.entries);
        match entries.get(key) {
            Some(Entry::Complete(value)) => Some(Arc::clone(value)),
            Some(Entry::InProgress(_)) | None => None,
        }
    }

    /// Runs or joins the proof obligation for `key`.
    ///
    /// The first top-level caller for a missing key owns `compute`. Other
    /// top-level callers for that key wait and observe the same published
    /// outcome. Calls made while the current thread is computing another key
    /// in this table are recursive. Such a call may own a missing key, but it
    /// never waits for a running key and returns
    /// [`ComponentWorkOutcome::RecursiveDependency`] instead.
    pub fn run<F>(&self, key: K, compute: F) -> ComponentWorkOutcome<V, I>
    where
        F: FnOnce(&ComponentWorkContext<'_, K, V, I>) -> ComponentWorkCompletion<V, I>,
    {
        let recursive = table_is_active(&self.identity);
        let running = {
            let mut entries = lock_unpoisoned(&self.entries);
            match entries.get(&key) {
                Some(Entry::Complete(value)) => {
                    return ComponentWorkOutcome::Complete(Arc::clone(value));
                }
                Some(Entry::InProgress(running)) => {
                    if recursive {
                        return ComponentWorkOutcome::RecursiveDependency;
                    }
                    Arc::clone(running)
                }
                None => {
                    let running = Arc::new(RunningWork::new());
                    entries.insert(key.clone(), Entry::InProgress(Arc::clone(&running)));
                    drop(entries);
                    return self.run_as_owner(key, running, compute);
                }
            }
        };
        running.wait()
    }

    fn run_as_owner<F>(
        &self,
        key: K,
        running: Arc<RunningWork<V, I>>,
        compute: F,
    ) -> ComponentWorkOutcome<V, I>
    where
        F: FnOnce(&ComponentWorkContext<'_, K, V, I>) -> ComponentWorkCompletion<V, I>,
    {
        let owner = WorkOwner {
            table: self,
            key,
            running,
            finished: false,
        };
        let _active = ActiveTableGuard::enter(&self.identity);
        let context = ComponentWorkContext { table: self };
        match compute(&context) {
            ComponentWorkCompletion::Complete(value) => owner.complete(value),
            ComponentWorkCompletion::Incomplete(reason) => owner.incomplete(reason),
        }
    }
}

/// Context passed to a component computation for safe recursive requests.
pub struct ComponentWorkContext<'table, K, V, I> {
    table: &'table ComponentWorkTable<K, V, I>,
}

impl<K, V, I> ComponentWorkContext<'_, K, V, I>
where
    K: Clone + Ord,
{
    /// Runs a dependent component proof through the same table.
    ///
    /// A dependency already in progress returns
    /// [`ComponentWorkOutcome::RecursiveDependency`] instead of waiting.
    pub fn run<F>(&self, key: K, compute: F) -> ComponentWorkOutcome<V, I>
    where
        F: FnOnce(&ComponentWorkContext<'_, K, V, I>) -> ComponentWorkCompletion<V, I>,
    {
        self.table.run(key, compute)
    }
}

struct WorkOwner<'table, K, V, I>
where
    K: Clone + Ord,
{
    table: &'table ComponentWorkTable<K, V, I>,
    key: K,
    running: Arc<RunningWork<V, I>>,
    finished: bool,
}

impl<K, V, I> WorkOwner<'_, K, V, I>
where
    K: Clone + Ord,
{
    fn complete(mut self, value: V) -> ComponentWorkOutcome<V, I> {
        let value = Arc::new(value);
        let installed = {
            let mut entries = lock_unpoisoned(&self.table.entries);
            let is_owner = matches!(
                entries.get(&self.key),
                Some(Entry::InProgress(running)) if Arc::ptr_eq(running, &self.running)
            );
            if is_owner {
                entries.insert(self.key.clone(), Entry::Complete(Arc::clone(&value)));
            }
            is_owner
        };
        self.finished = true;
        if installed {
            self.running
                .publish(PublishedOutcome::Complete(Arc::clone(&value)));
            ComponentWorkOutcome::Complete(value)
        } else {
            self.running.publish(PublishedOutcome::Abandoned);
            ComponentWorkOutcome::Abandoned
        }
    }

    fn incomplete(mut self, reason: I) -> ComponentWorkOutcome<V, I> {
        self.remove_owned_entry();
        self.finished = true;
        let reason = Arc::new(reason);
        self.running
            .publish(PublishedOutcome::Incomplete(Arc::clone(&reason)));
        ComponentWorkOutcome::Incomplete(reason)
    }

    fn remove_owned_entry(&self) {
        let mut entries = lock_unpoisoned(&self.table.entries);
        let is_owner = matches!(
            entries.get(&self.key),
            Some(Entry::InProgress(running)) if Arc::ptr_eq(running, &self.running)
        );
        if is_owner {
            entries.remove(&self.key);
        }
    }
}

impl<K, V, I> Drop for WorkOwner<'_, K, V, I>
where
    K: Clone + Ord,
{
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.remove_owned_entry();
        self.running.publish(PublishedOutcome::Abandoned);
    }
}

struct ActiveTableGuard {
    identity: Arc<TableIdentity>,
}

impl ActiveTableGuard {
    fn enter(identity: &Arc<TableIdentity>) -> Self {
        ACTIVE_TABLES.with(|active| active.borrow_mut().push(Arc::clone(identity)));
        Self {
            identity: Arc::clone(identity),
        }
    }
}

impl Drop for ActiveTableGuard {
    fn drop(&mut self) {
        ACTIVE_TABLES.with(|active| {
            let popped = active.borrow_mut().pop();
            debug_assert!(
                popped
                    .as_ref()
                    .is_some_and(|identity| Arc::ptr_eq(identity, &self.identity)),
                "active component work tables must unwind in stack order"
            );
        });
    }
}

fn table_is_active(identity: &Arc<TableIdentity>) -> bool {
    ACTIVE_TABLES.with(|active| {
        active
            .borrow()
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, identity))
    })
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc, Barrier, Condvar, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use super::{
        ComponentWorkCompletion, ComponentWorkOutcome, ComponentWorkState, ComponentWorkTable,
        lock_unpoisoned,
    };

    #[test]
    fn concurrent_same_key_has_one_owner_and_one_value() {
        const WORKERS: usize = 8;
        let table = Arc::new(ComponentWorkTable::<u32, u32, &'static str>::new());
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(WORKERS + 1));

        let handles = (0..WORKERS)
            .map(|_| {
                let table = Arc::clone(&table);
                let entered = Arc::clone(&entered);
                let release = Arc::clone(&release);
                let calls = Arc::clone(&calls);
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    start.wait();
                    table.run(17, |_| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let (lock, ready) = &*entered;
                        *lock_unpoisoned(lock) = true;
                        ready.notify_one();

                        let (lock, ready) = &*release;
                        let mut released = lock_unpoisoned(lock);
                        while !*released {
                            released = ready
                                .wait(released)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        ComponentWorkCompletion::Complete(29)
                    })
                })
            })
            .collect::<Vec<_>>();

        start.wait();
        let (lock, ready) = &*entered;
        let mut owner_entered = lock_unpoisoned(lock);
        while !*owner_entered {
            owner_entered = ready
                .wait(owner_entered)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(owner_entered);
        assert_eq!(table.state(&17), Some(ComponentWorkState::InProgress));

        let (lock, ready) = &*release;
        *lock_unpoisoned(lock) = true;
        ready.notify_all();

        for handle in handles {
            let outcome = handle.join().expect("worker must not panic");
            assert_eq!(outcome.complete_value(), Some(&29));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(table.state(&17), Some(ComponentWorkState::Complete));
    }

    #[test]
    fn direct_recursive_dependency_never_runs_or_waits() {
        let table = ComponentWorkTable::<&str, u32, ()>::new();
        let outcome = table.run("same", |context| {
            assert_eq!(
                context.run("same", |_| panic!("recursive work must not run")),
                ComponentWorkOutcome::RecursiveDependency
            );
            ComponentWorkCompletion::Complete(1)
        });
        assert_eq!(outcome.complete_value(), Some(&1));
    }

    #[test]
    fn indirect_recursive_dependency_falls_back_at_cycle_edge() {
        let table = ComponentWorkTable::<&str, u32, ()>::new();
        let outcome = table.run("a", |a_context| {
            let b = a_context.run("b", |b_context| {
                assert_eq!(
                    b_context.run("a", |_| panic!("cycle edge must not run")),
                    ComponentWorkOutcome::RecursiveDependency
                );
                ComponentWorkCompletion::Complete(2)
            });
            assert_eq!(b.complete_value(), Some(&2));
            ComponentWorkCompletion::Complete(1)
        });

        assert_eq!(outcome.complete_value(), Some(&1));
        assert_eq!(
            table.ordered_states(),
            vec![
                ("a", ComponentWorkState::Complete),
                ("b", ComponentWorkState::Complete),
            ]
        );
    }

    #[test]
    fn cross_thread_dependency_cycle_never_deadlocks() {
        let table = Arc::new(ComponentWorkTable::<&str, u32, ()>::new());
        let both_owned = Arc::new(Barrier::new(2));
        let dependencies_checked = Arc::new(Barrier::new(2));
        let handles = [("a", "b", 1), ("b", "a", 2)].map(|(own, dependency, value)| {
            let table = Arc::clone(&table);
            let both_owned = Arc::clone(&both_owned);
            let dependencies_checked = Arc::clone(&dependencies_checked);
            thread::spawn(move || {
                table.run(own, |context| {
                    both_owned.wait();
                    let dependency_outcome =
                        context.run(dependency, |_| panic!("running dependency must not run"));
                    dependencies_checked.wait();
                    assert_eq!(
                        dependency_outcome,
                        ComponentWorkOutcome::RecursiveDependency
                    );
                    ComponentWorkCompletion::Complete(value)
                })
            })
        });

        for (handle, expected) in handles.into_iter().zip([1, 2]) {
            assert_eq!(
                handle
                    .join()
                    .expect("cycle test worker must not panic")
                    .complete_value(),
                Some(&expected)
            );
        }
    }

    #[test]
    fn incomplete_attempt_is_shared_but_not_cached() {
        let table = ComponentWorkTable::<u32, u32, &'static str>::new();
        assert_eq!(
            table.run(4, |_| ComponentWorkCompletion::Incomplete("cancelled")),
            ComponentWorkOutcome::Incomplete(Arc::new("cancelled"))
        );
        assert_eq!(table.state(&4), None);
        assert_eq!(table.cached_complete(&4), None);

        let complete = table.run(4, |_| ComponentWorkCompletion::Complete(8));
        assert_eq!(complete.complete_value(), Some(&8));
        assert_eq!(table.state(&4), Some(ComponentWorkState::Complete));
    }

    #[test]
    fn owner_panic_cleans_up_for_a_later_attempt() {
        let table = ComponentWorkTable::<u32, u32, ()>::new();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            table.run(9, |_| -> ComponentWorkCompletion<u32, ()> {
                panic!("simulated optimizer panic");
            });
        }));
        assert!(panic.is_err());
        assert_eq!(table.state(&9), None);

        let retry = table.run(9, |_| ComponentWorkCompletion::Complete(11));
        assert_eq!(retry.complete_value(), Some(&11));
    }

    #[test]
    fn state_snapshot_order_is_key_order_not_completion_order() {
        let table = ComponentWorkTable::<u32, u32, ()>::new();
        for key in [9, 2, 5] {
            assert!(matches!(
                table.run(key, |_| ComponentWorkCompletion::Complete(key * 2)),
                ComponentWorkOutcome::Complete(_)
            ));
        }
        assert_eq!(
            table.ordered_states(),
            vec![
                (2, ComponentWorkState::Complete),
                (5, ComponentWorkState::Complete),
                (9, ComponentWorkState::Complete),
            ]
        );
    }
}
