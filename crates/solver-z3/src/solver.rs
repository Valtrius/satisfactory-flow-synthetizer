//! Exact load-balancer search: SMT encoding + portfolio Z3 solving.
//!
//! # Search shape
//!
//! [`solve_exact`] walks node counts from a combinatorial lower bound upward. At each
//! fixed size `N` it must either find a verified topology or prove every operator
//! *profile* (multiset of splitter/merger kinds) unsatisfiable before trying `N+1`.
//! That keeps minimality: speculative larger sizes are never started while `N` is open.
//!
//! When `SolveRequest::enumerate_all_at_n` is set, the first verified size is kept and
//! every distinct layout at that size is collected (with streaming
//! [`SolverEvent::SolutionFound`] events) instead of returning the first hit.
//!
//! # Portfolio parallelism
//!
//! Within one size, work is scheduled by **profile** (the proof obligation) and
//! **attempt** (a disposable Z3 solve of that profile with its own seed and thread
//! budget). CPU slots are filled by giving every unresolved profile one attempt first,
//! then round-robin seeded replicas. Empirically, a few medium-width Z3 instances
//! (`T ≤ 4`) beat one fat solver or a swarm of 1-thread replicas on the hard cyclic
//! cases; see [`default_threads_per_attempt`].
//!
//! Intentional interrupts (user cancel, sibling found a solution, profile already
//! proven UNSAT) surface as cancelled attempts. Unexpected Z3 `Unknown` only abandons
//! that replica; the portfolio fails only when every unresolved profile exhausts its
//! abandon allowance with nothing still running.
//!
//! # Encoding layers
//!
//! Each attempt builds an SMT formula with:
//! - exact `Real` belt flows and routing Bools (`QF_LRA` when possible);
//! - port-order symmetry breaking and reachability ranks;
//! - for single-input problems with small prime factors in transfer denominators, a
//!   modular residue witness (Bool + BV) that steers Z3 toward valid cyclic topologies.
//!
//! SAT models are always checked by [`crate::verify::verify_candidate`]; unstable
//! feedback candidates are blocked and the attempt continues.

use std::{
    collections::HashSet,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use num::{BigInt, BigRational, Integer, One, Zero};
use thiserror::Error;
use z3::{
    Config, Context, Model, Params, SatResult, Solver,
    ast::{BV, Bool, Real},
    with_z3_config,
};

use crate::{
    format_rate,
    model::{
        Candidate, OperatorKind, PORTS, Problem, Route, Solution, SolveRequest, SolverProgress,
        consumer_count, node_consumer, node_producer, output_consumer, producer_count,
    },
    verify::{
        ProblemError, build_solution, normalize_problem, solution_identity, verify_candidate,
    },
};

#[derive(Clone, Debug, Error)]
pub enum SolveError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("not enough input: {available} available, {requested} requested")]
    InsufficientMaterial {
        available: String,
        requested: String,
    },
    #[error("exact solver failed: {0}")]
    Solver(String),
}

#[derive(Clone, Debug)]
pub enum SolveTermination {
    /// Classic mode: the first verified minimal-N layout.
    Completed(Box<Solution>),
    /// Full-N mode finished collecting every distinct layout at minimal N.
    Enumerated(Vec<Solution>),
    /// Full-N found layouts but the portfolio failed before profiles were exhausted.
    Incomplete {
        solutions: Vec<Solution>,
        error: SolveError,
    },
    /// Search stopped by the user. `solutions` is non-empty only for partial full-N runs.
    Cancelled { solutions: Vec<Solution> },
}

#[derive(Clone, Debug)]
pub enum SolverEvent {
    Progress(SolverProgress),
    /// Emitted as each unique layout is found during full-N enumeration.
    SolutionFound(Box<Solution>),
}

/// Find the smallest verified exact topology, unless the caller cancels the search.
///
/// Sizes are searched sequentially from [`node_count_lower_bound`]. Parallelism lives
/// entirely inside each size (see module docs). Progress callbacks see rejected
/// unstable candidates aggregated across portfolio replicas at that size — telemetry
/// only; over-counting across replicas is possible and harmless.
///
/// Mid-size [`SolverProgress::SizeProgress`] events are throttled so concurrent
/// portfolio workers do not flood the UI bridge.
///
/// # Errors
///
/// Returns an error for invalid rates, insufficient input, or an unexpected solver failure.
pub fn solve_exact<F>(
    request: &SolveRequest,
    cancel: &AtomicBool,
    on_progress: F,
) -> Result<SolveTermination, SolveError>
where
    F: FnMut(SolverEvent) + Send,
{
    let problem = normalize_problem(request).map_err(|error| match error {
        ProblemError::Invalid(message) => SolveError::InvalidRequest(message),
        ProblemError::Insufficient {
            available,
            requested,
        } => SolveError::InsufficientMaterial {
            available: format_rate(&available).exact,
            requested: format_rate(&requested).exact,
        },
    })?;

    let lower_bound = node_count_lower_bound(&problem);
    let mut node_count = lower_bound;
    let mut rejected_total = 0;
    let on_progress = Mutex::new(on_progress);
    let throttle = EmitThrottle::new(Duration::from_millis(150));
    let enumerate = request.enumerate_all_at_n;

    report_progress(&on_progress, SolverProgress::Preparing { lower_bound });

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(SolveTermination::Cancelled {
                solutions: Vec::new(),
            });
        }

        match search_size(
            &problem,
            cancel,
            &on_progress,
            &throttle,
            SizeSearchOptions {
                node_count,
                lower_bound,
                rejected_before: rejected_total,
                enumerate,
            },
        )? {
            SizeSearch::Found(candidate) => {
                let solution = build_solution(&problem, &candidate);
                return Ok(SolveTermination::Completed(Box::new(solution)));
            }
            SizeSearch::Enumerated(solutions) => {
                return Ok(SolveTermination::Enumerated(solutions));
            }
            SizeSearch::FailedEnumerated { solutions, error } => {
                return Ok(SolveTermination::Incomplete { solutions, error });
            }
            SizeSearch::CancelledEnumerated(solutions) => {
                return Ok(SolveTermination::Cancelled { solutions });
            }
            SizeSearch::Unsatisfiable(rejected) => {
                rejected_total += rejected;
                if rejected > 0 {
                    report_progress(
                        &on_progress,
                        SolverProgress::CandidateRejected {
                            node_count,
                            rejected_unstable_candidates: rejected_total,
                            reason: "all algebraic candidates at this size had unstable or ambiguous feedback"
                                .to_owned(),
                            lower_bound,
                        },
                    );
                }
                node_count = node_count
                    .checked_add(1)
                    .ok_or_else(|| SolveError::Solver("node count overflowed".to_owned()))?;
            }
            SizeSearch::Cancelled => {
                return Ok(SolveTermination::Cancelled {
                    solutions: Vec::new(),
                });
            }
        }
    }
}

fn report_progress<F>(on_progress: &Mutex<F>, progress: SolverProgress)
where
    F: FnMut(SolverEvent) + Send,
{
    (on_progress.lock().expect("progress callback"))(SolverEvent::Progress(progress));
}

/// Limits mid-size progress spam from concurrent portfolio workers.
///
/// Identical heartbeats are time-throttled. Counter changes (active/launched/unsat/…)
/// always emit so the UI sees the initial claim wave jump to the full slot count.
type ProgressSignature = (usize, usize, usize, usize, usize);

struct EmitThrottle {
    last_emit: Mutex<Option<Instant>>,
    min_gap: Duration,
    last_sig: Mutex<Option<ProgressSignature>>,
}

impl EmitThrottle {
    fn new(min_gap: Duration) -> Self {
        Self {
            last_emit: Mutex::new(None),
            min_gap,
            last_sig: Mutex::new(None),
        }
    }

    fn allow(&self, force: bool, sig: ProgressSignature) -> bool {
        let mut last_sig = self.last_sig.lock().expect("progress signature");
        let changed = *last_sig != Some(sig);
        if changed {
            *last_sig = Some(sig);
        }
        let mut last = self.last_emit.lock().expect("progress throttle");
        let now = Instant::now();
        if force || changed {
            *last = Some(now);
            return true;
        }
        if let Some(previous) = *last
            && now.duration_since(previous) < self.min_gap
        {
            return false;
        }
        *last = Some(now);
        true
    }
}

/// Smallest node count that can possibly realize the required branching / port balance.
///
/// Combines the reduced transfer denominator [`required_denominator`] (how many ×2/×3
/// splits are needed, and whether a feedback merger is mandatory) with belt/port
/// accounting for outputs and discard lines. This is a lower bound only: SMT may still
/// need a larger `N`.
fn node_count_lower_bound(problem: &Problem) -> usize {
    let q = required_denominator(problem);
    let input_count = problem.inputs.len();
    let output_count = problem.outputs.len();
    let discard_min = if problem.discard_rate.is_zero() {
        0
    } else {
        minimum_discard_belts(problem)
    };
    let needs_feedback = !is_two_three_smooth(&q);
    let extra_outputs = output_count.saturating_sub(input_count) + discard_min;
    let splitter_2_max = ceil_log(&q, 2).max(extra_outputs).saturating_add(1);
    let splitter_3_max = ceil_log(&q, 3)
        .max(extra_outputs.div_ceil(2))
        .saturating_add(1);

    let mut best = None;
    for splitter_2 in 0..=splitter_2_max {
        for splitter_3 in 0..=splitter_3_max {
            if !arity_product_covers(splitter_2, splitter_3, &q) {
                continue;
            }
            let Some(mergers) = minimum_mergers(
                splitter_2,
                splitter_3,
                input_count,
                output_count,
                discard_min,
                needs_feedback,
            ) else {
                continue;
            };
            let total = splitter_2 + splitter_3 + mergers;
            best = Some(best.map_or(total, |current: usize| current.min(total)));
        }
    }
    best.unwrap_or_else(|| extra_outputs.div_ceil(2))
}

/// Reduced integer `q` such that every endpoint rate is an integer multiple of
/// `total_input / q` after clearing a common grain.
///
/// Splitter arities only multiply denominators by 2 or 3, so if `q` has any other prime
/// factor the topology needs a feedback loop (and modular filters may apply).
fn required_denominator(problem: &Problem) -> BigInt {
    let mut scale = BigInt::one();
    for rate in problem
        .inputs
        .iter()
        .map(|endpoint| &endpoint.rate)
        .chain(problem.outputs.iter().map(|endpoint| &endpoint.rate))
        .chain(std::iter::once(&problem.discard_rate))
    {
        if !rate.denom().is_zero() {
            scale = scale.lcm(rate.denom());
        }
    }
    let to_integer =
        |rate: &BigRational| (rate * BigRational::from_integer(scale.clone())).to_integer();
    let mut input_gcd = BigInt::zero();
    for endpoint in &problem.inputs {
        let rate = to_integer(&endpoint.rate);
        input_gcd = if input_gcd.is_zero() {
            rate
        } else {
            input_gcd.gcd(&rate)
        };
    }
    if input_gcd.is_zero() {
        return BigInt::one();
    }

    let mut grain = input_gcd.clone();
    for endpoint in &problem.outputs {
        grain = grain.gcd(&to_integer(&endpoint.rate));
    }
    if !problem.discard_rate.is_zero() {
        grain = grain.gcd(&to_integer(&problem.discard_rate));
    }
    input_gcd / grain
}

/// True when `value`'s only prime factors are 2 and 3 (achievable by splitters alone).
fn is_two_three_smooth(value: &BigInt) -> bool {
    if value.is_zero() {
        return false;
    }
    let mut remaining = value.clone();
    while (&remaining % 2_u8).is_zero() {
        remaining /= 2_u8;
    }
    while (&remaining % 3_u8).is_zero() {
        remaining /= 3_u8;
    }
    remaining.is_one()
}

fn ceil_log(value: &BigInt, base: u8) -> usize {
    if *value <= BigInt::one() {
        return 0;
    }
    let base = BigInt::from(base);
    let mut power = BigInt::one();
    let mut exponent = 0;
    while power < *value {
        power *= &base;
        exponent += 1;
    }
    exponent
}

/// Whether `2^splitter_2 * 3^splitter_3` is large enough to cover denominator `q`.
fn arity_product_covers(splitter_2: usize, splitter_3: usize, q: &BigInt) -> bool {
    if *q <= BigInt::one() {
        return true;
    }
    let mut product = BigInt::one();
    for _ in 0..splitter_2 {
        product *= 2_u8;
        if product >= *q {
            return true;
        }
    }
    for _ in 0..splitter_3 {
        product *= 3_u8;
        if product >= *q {
            return true;
        }
    }
    product >= *q
}

/// Minimum mergers needed for port balance given a splitter mix (and optional feedback).
fn minimum_mergers(
    splitter_2: usize,
    splitter_3: usize,
    input_count: usize,
    output_count: usize,
    discard_min: usize,
    needs_feedback: bool,
) -> Option<usize> {
    let branch_excess = isize::try_from(splitter_2).ok()? + 2 * isize::try_from(splitter_3).ok()?
        - (isize::try_from(output_count).ok()? - isize::try_from(input_count).ok()?);
    if discard_min == 0 {
        if branch_excess < 0 {
            return None;
        }
        let weighted = usize::try_from(branch_excess).ok()?;
        if needs_feedback && weighted == 0 {
            return None;
        }
        let mut mergers = weighted.div_ceil(2);
        if needs_feedback {
            mergers = mergers.max(1);
        }
        Some(mergers)
    } else if branch_excess < isize::try_from(discard_min).ok()? {
        None
    } else if needs_feedback {
        if usize::try_from(branch_excess).ok()? < discard_min + 1 {
            None
        } else {
            Some(1)
        }
    } else {
        Some(0)
    }
}

fn minimum_discard_belts(problem: &Problem) -> usize {
    let mut remaining = problem.discard_rate.clone();
    let mut belts = 0;
    while remaining > num::BigRational::zero() {
        remaining -= &problem.belt_rate;
        belts += 1;
    }
    belts
}

/// Outcome of searching all profiles at one fixed node count.
enum SizeSearch {
    Found(crate::model::VerifiedCandidate),
    /// Full-N mode: every distinct layout found at this size (profiles exhausted).
    Enumerated(Vec<Solution>),
    /// Full-N found layouts, then the portfolio stuck/failed with profiles still open.
    FailedEnumerated {
        solutions: Vec<Solution>,
        error: SolveError,
    },
    /// Full-N mode cancelled after at least one layout was found.
    CancelledEnumerated(Vec<Solution>),
    /// Proven unsat; the `usize` is how many SAT models failed verification on the
    /// attempt that closed a profile (aggregated across profiles — approximate).
    Unsatisfiable(usize),
    Cancelled,
}

impl SizeSearch {
    #[cfg(test)]
    fn label(&self) -> &'static str {
        match self {
            Self::Found(_) => "found",
            Self::Enumerated(_) => "enumerated",
            Self::FailedEnumerated { .. } => "failed_enumerated",
            Self::CancelledEnumerated(_) => "cancelled_enumerated",
            Self::Unsatisfiable(_) => "unsat",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Decide how a full-N size search ends after workers stop.
fn finish_enumerate_size(
    cancel: bool,
    solutions: Vec<Solution>,
    portfolio_error: Option<SolveError>,
    rejected: usize,
) -> Result<SizeSearch, SolveError> {
    if cancel {
        if solutions.is_empty() {
            return Ok(SizeSearch::Cancelled);
        }
        return Ok(SizeSearch::CancelledEnumerated(solutions));
    }
    if let Some(error) = portfolio_error {
        // Portfolio stuck/failed while profiles remain — never claim Enumerated/complete.
        if solutions.is_empty() {
            return Err(error);
        }
        return Ok(SizeSearch::FailedEnumerated { solutions, error });
    }
    if !solutions.is_empty() {
        return Ok(SizeSearch::Enumerated(solutions));
    }
    Ok(SizeSearch::Unsatisfiable(rejected))
}

/// Z3 threads per portfolio attempt: prefer 4 (hard-case sweet spot), but never so fat
/// that independent profiles cannot each occupy a slot.
///
/// Examples on 32 CPUs: 1 profile → T=4 (8 attempts); 16 profiles → T=2 (16 attempts);
/// 32 profiles → T=1. More CPUs do **not** raise T above 4 — fattening a single Z3 past
/// that did not help in benchmarks.
fn default_threads_per_attempt(available: usize, profile_count: usize) -> usize {
    const PREFERRED_Z3_THREADS: usize = 4;
    PREFERRED_Z3_THREADS.min((available / profile_count.max(1)).max(1))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileStatus {
    Unresolved,
    Unsat,
}

/// Tracks proof obligations (profiles) and disposable attempt launches for one size.
///
/// Workers call [`PortfolioScheduler::claim`] for `(profile, replica)` pairs. A profile
/// is closed on the first UNSAT attempt; siblings are interrupted via a per-profile flag
/// owned by the search loop. Verified SAT closes the whole size.
struct PortfolioScheduler {
    status: Vec<ProfileStatus>,
    /// Attempts started for each profile (including finished ones).
    launched: Vec<usize>,
    /// Currently running attempts per profile.
    active: Vec<usize>,
    /// Unexpected Z3 `Unknown` count per profile (not interrupts).
    abandoned: Vec<usize>,
    unresolved: usize,
}

impl PortfolioScheduler {
    fn new(profile_count: usize) -> Self {
        Self {
            status: vec![ProfileStatus::Unresolved; profile_count],
            launched: vec![0; profile_count],
            active: vec![0; profile_count],
            abandoned: vec![0; profile_count],
            unresolved: profile_count,
        }
    }

    /// Next attempt to run, or `None` if every unresolved profile is abandon-capped or done.
    fn claim(&mut self, abandon_cap: usize) -> Option<(usize, usize)> {
        if self.unresolved == 0 {
            return None;
        }
        let eligible = |profile: usize| {
            self.status[profile] == ProfileStatus::Unresolved
                && self.abandoned[profile] < abandon_cap
        };
        // Priority 1: every unresolved profile gets a first attempt.
        if let Some(profile) =
            (0..self.status.len()).find(|&profile| eligible(profile) && self.launched[profile] == 0)
        {
            let replica = 0;
            self.launched[profile] = 1;
            self.active[profile] += 1;
            return Some((profile, replica));
        }
        // Priority 2/3: round-robin replicas across unresolved profiles (fewest launched).
        let profile = (0..self.status.len())
            .filter(|&profile| eligible(profile))
            .min_by_key(|&profile| (self.launched[profile], profile))?;
        let replica = self.launched[profile];
        self.launched[profile] += 1;
        self.active[profile] += 1;
        Some((profile, replica))
    }

    fn release_active(&mut self, profile: usize) {
        self.active[profile] = self.active[profile].saturating_sub(1);
    }

    fn note_abandon(&mut self, profile: usize) {
        self.abandoned[profile] += 1;
    }

    fn mark_unsat(&mut self, profile: usize) -> bool {
        if self.status[profile] != ProfileStatus::Unresolved {
            return false;
        }
        self.status[profile] = ProfileStatus::Unsat;
        self.unresolved -= 1;
        true
    }

    fn all_unsat(&self) -> bool {
        self.unresolved == 0
    }

    fn telemetry(&self) -> PortfolioTelemetry {
        PortfolioTelemetry {
            profiles_total: self.status.len(),
            profiles_unresolved: self.unresolved,
            profiles_unsat: self
                .status
                .iter()
                .filter(|status| **status == ProfileStatus::Unsat)
                .count(),
            active_attempts: self.active.iter().sum(),
            launched_attempts: self.launched.iter().sum(),
            abandoned_attempts: self.abandoned.iter().sum(),
        }
    }

    /// True when unresolved work remains but nothing is running and every open profile
    /// has hit `abandon_cap` unexpected Unknowns — safe to fail the size search.
    fn portfolio_stuck(&self, abandon_cap: usize) -> bool {
        if self.unresolved == 0 {
            return false;
        }
        let any_active = self.active.iter().sum::<usize>() > 0;
        if any_active {
            return false;
        }
        (0..self.status.len()).all(|profile| {
            self.status[profile] != ProfileStatus::Unresolved
                || self.abandoned[profile] >= abandon_cap
        })
    }
}

struct PortfolioTelemetry {
    profiles_total: usize,
    profiles_unresolved: usize,
    profiles_unsat: usize,
    active_attempts: usize,
    launched_attempts: usize,
    abandoned_attempts: usize,
}

struct SizeProgressContext<'a, F: FnMut(SolverEvent) + Send> {
    throttle: &'a EmitThrottle,
    on_progress: &'a Mutex<F>,
    scheduler: &'a Mutex<PortfolioScheduler>,
    rejected: &'a AtomicUsize,
    node_count: usize,
    lower_bound: usize,
    rejected_before: usize,
    attempt_slots: usize,
}

fn report_size_progress<F>(force: bool, ctx: &SizeProgressContext<'_, F>)
where
    F: FnMut(SolverEvent) + Send,
{
    // Read counters first so claim-wave jumps (1→8 active) always emit even when
    // many workers report inside the throttle window.
    let telemetry = ctx.scheduler.lock().expect("scheduler").telemetry();
    let rejected = ctx.rejected_before + ctx.rejected.load(Ordering::Relaxed);
    let sig = (
        ctx.node_count,
        telemetry.active_attempts,
        telemetry.launched_attempts,
        telemetry.profiles_unsat,
        telemetry.abandoned_attempts + rejected,
    );
    // `force` is for meaningful transitions (profile closed, found, abandoned,
    // cancel/fail flush so active attempts can reach zero).
    // Routine identical heartbeats go through the time throttle.
    if !ctx.throttle.allow(force, sig) {
        return;
    }
    report_progress(
        ctx.on_progress,
        SolverProgress::SizeProgress {
            node_count: ctx.node_count,
            lower_bound: ctx.lower_bound,
            profiles_total: telemetry.profiles_total,
            profiles_unresolved: telemetry.profiles_unresolved,
            profiles_unsat: telemetry.profiles_unsat,
            active_attempts: telemetry.active_attempts,
            launched_attempts: telemetry.launched_attempts,
            abandoned_attempts: telemetry.abandoned_attempts,
            rejected_unstable_candidates: rejected,
            attempt_slots: ctx.attempt_slots,
        },
    );
}

/// Deterministic portfolio diversity: same `(N, profile, replica)` → same Z3 seeds.
fn attempt_seed(node_count: usize, profile: usize, replica: usize) -> u32 {
    let mut hash = 0x9e37_79b9_7f4a_7c15_u64;
    hash ^= node_count as u64;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= (profile as u64).wrapping_shl(1);
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^= (replica as u64).wrapping_shl(2);
    u32::try_from(hash & u64::from(u32::MAX)).expect("masked to u32")
}

/// Search all operator profiles at `node_count` under the production portfolio policy.
#[derive(Clone, Copy)]
struct SizeSearchOptions {
    node_count: usize,
    lower_bound: usize,
    rejected_before: usize,
    enumerate: bool,
}

fn search_size<F>(
    problem: &Problem,
    cancel: &AtomicBool,
    on_progress: &Mutex<F>,
    throttle: &EmitThrottle,
    options: SizeSearchOptions,
) -> Result<SizeSearch, SolveError>
where
    F: FnMut(SolverEvent) + Send,
{
    let profiles = operator_profiles(problem, options.node_count);
    let available = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let threads_per_attempt = if profiles.is_empty() {
        1
    } else {
        default_threads_per_attempt(available, profiles.len())
    };
    search_size_with_threads_per_attempt(
        problem,
        options.node_count,
        cancel,
        threads_per_attempt,
        Some(profiles),
        on_progress,
        throttle,
        options.lower_bound,
        options.rejected_before,
        options.enumerate,
    )
}

/// Portfolio search at one size with an explicit Z3 thread budget per attempt.
///
/// Spawns `floor(cpus / threads_per_attempt)` workers. Each worker repeatedly claims
/// attempts until the size is solved, all profiles are UNSAT, the user cancels, or the
/// portfolio is stuck on Unknowns.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn search_size_with_threads_per_attempt<F>(
    problem: &Problem,
    node_count: usize,
    cancel: &AtomicBool,
    threads_per_attempt: usize,
    profiles: Option<Vec<Vec<OperatorKind>>>,
    on_progress: &Mutex<F>,
    throttle: &EmitThrottle,
    lower_bound: usize,
    rejected_before: usize,
    enumerate: bool,
) -> Result<SizeSearch, SolveError>
where
    F: FnMut(SolverEvent) + Send,
{
    let threads_per_attempt = threads_per_attempt.max(1);
    let profiles = match profiles {
        Some(profiles) => profiles,
        None => operator_profiles(problem, node_count),
    };

    let available = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let attempt_slots = if profiles.is_empty() {
        0
    } else {
        (available / threads_per_attempt).max(1)
    };

    report_progress(
        on_progress,
        SolverProgress::Checking {
            node_count,
            rejected_unstable_candidates: rejected_before,
            lower_bound,
            profile_count: profiles.len(),
            attempt_slots,
            threads_per_attempt,
        },
    );

    if profiles.is_empty() {
        return Ok(SizeSearch::Unsatisfiable(0));
    }

    // Cap unexpected Unknowns per profile roughly at one wave of concurrent attempts.
    // Workers may already have replacements in flight when the cap is hit.
    let abandon_cap = attempt_slots;
    let scheduler = Mutex::new(PortfolioScheduler::new(profiles.len()));
    let stop_profile = (0..profiles.len())
        .map(|_| AtomicBool::new(false))
        .collect::<Vec<_>>();
    let stop_all = AtomicBool::new(false);
    let rejected = AtomicUsize::new(0);
    let found = Mutex::new(None);
    let enumerated = Mutex::new(Vec::<Solution>::new());
    let seen_identities = Mutex::new(HashSet::<String>::new());
    let error = Mutex::new(None);
    let last_abandon_reason = Mutex::new(None);
    let progress_ctx = SizeProgressContext {
        throttle,
        on_progress,
        scheduler: &scheduler,
        rejected: &rejected,
        node_count,
        lower_bound,
        rejected_before,
        attempt_slots,
    };
    let profiles = profiles.as_slice();
    let progress_ctx = &progress_ctx;
    let stop_profile = stop_profile.as_slice();
    let stop_all = &stop_all;
    let scheduler = &scheduler;
    let found = &found;
    let enumerated = &enumerated;
    let seen_identities = &seen_identities;
    let error = &error;
    let last_abandon_reason = &last_abandon_reason;

    thread::scope(|scope| {
        for _slot in 0..attempt_slots {
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) || stop_all.load(Ordering::Relaxed) {
                        break;
                    }
                    let claimed = scheduler.lock().expect("scheduler").claim(abandon_cap);
                    let Some((profile, replica)) = claimed else {
                        let sched = scheduler.lock().expect("scheduler");
                        if sched.portfolio_stuck(abandon_cap) {
                            let reason = last_abandon_reason
                                .lock()
                                .expect("abandon reason")
                                .clone()
                                .unwrap_or_else(|| "Z3 returned unknown".to_owned());
                            let mut slot_err = error.lock().expect("error lock");
                            if slot_err.is_none() {
                                *slot_err = Some(SolveError::Solver(format!(
                                    "all portfolio attempts abandoned with unknown: {reason}"
                                )));
                            }
                            stop_all.store(true, Ordering::Relaxed);
                        }
                        break;
                    };
                    report_size_progress(false, progress_ctx);
                    if stop_profile[profile].load(Ordering::Relaxed) {
                        scheduler.lock().expect("scheduler").release_active(profile);
                        report_size_progress(false, progress_ctx);
                        continue;
                    }
                    let stop_this_profile = &stop_profile[profile];
                    let outcome = run_attempt(
                        problem,
                        profiles[profile].as_slice(),
                        node_count,
                        profile,
                        replica,
                        threads_per_attempt,
                        cancel,
                        stop_all,
                        stop_this_profile,
                        progress_ctx,
                        enumerate,
                        enumerated,
                        seen_identities,
                    );
                    {
                        let mut sched = scheduler.lock().expect("scheduler");
                        sched.release_active(profile);
                        match &outcome {
                            AttemptResult::Abandoned(_) => sched.note_abandon(profile),
                            AttemptResult::Unsat => {
                                let first = sched.mark_unsat(profile);
                                if first {
                                    stop_profile[profile].store(true, Ordering::Relaxed);
                                }
                            }
                            _ => {}
                        }
                        if sched.portfolio_stuck(abandon_cap) {
                            let reason = match &outcome {
                                AttemptResult::Abandoned(reason) => reason.clone(),
                                _ => last_abandon_reason
                                    .lock()
                                    .expect("abandon reason")
                                    .clone()
                                    .unwrap_or_else(|| "Z3 returned unknown".to_owned()),
                            };
                            let mut slot_err = error.lock().expect("error lock");
                            if slot_err.is_none() {
                                *slot_err = Some(SolveError::Solver(format!(
                                    "all portfolio attempts abandoned with unknown: {reason}"
                                )));
                            }
                            stop_all.store(true, Ordering::Relaxed);
                        } else if sched.all_unsat() {
                            stop_all.store(true, Ordering::Relaxed);
                        }
                    }
                    match outcome {
                        AttemptResult::Found(candidate) => {
                            {
                                let mut found_slot = found.lock().expect("found lock");
                                if found_slot.is_none() {
                                    *found_slot = Some(candidate);
                                }
                            }
                            stop_all.store(true, Ordering::Relaxed);
                            report_size_progress(true, progress_ctx);
                            break;
                        }
                        AttemptResult::Unsat => {
                            report_size_progress(true, progress_ctx);
                            if stop_all.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        AttemptResult::Cancelled => {
                            report_size_progress(true, progress_ctx);
                            if cancel.load(Ordering::Relaxed) || stop_all.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        AttemptResult::Abandoned(reason) => {
                            *last_abandon_reason.lock().expect("abandon reason") = Some(reason);
                            report_size_progress(true, progress_ctx);
                            if stop_all.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        AttemptResult::Failed(attempt_error) => {
                            let mut slot_err = error.lock().expect("error lock");
                            if slot_err.is_none() {
                                *slot_err = Some(attempt_error);
                            }
                            stop_all.store(true, Ordering::Relaxed);
                            report_size_progress(true, progress_ctx);
                            break;
                        }
                    }
                }
            });
        }
    });

    if let Some(candidate) = found.lock().expect("found lock").take() {
        return Ok(SizeSearch::Found(candidate));
    }

    let enumerated_solutions = std::mem::take(&mut *enumerated.lock().expect("enumerated lock"));
    let portfolio_error = error.lock().expect("error lock").take();

    if enumerate {
        return finish_enumerate_size(
            cancel.load(Ordering::Relaxed),
            enumerated_solutions,
            portfolio_error,
            rejected.load(Ordering::Relaxed),
        );
    }

    if let Some(error) = portfolio_error {
        return Err(error);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(SizeSearch::Cancelled);
    }
    Ok(SizeSearch::Unsatisfiable(rejected.load(Ordering::Relaxed)))
}

/// Result of one disposable Z3 attempt on a single profile.
enum AttemptResult {
    Found(crate::model::VerifiedCandidate),
    Unsat,
    /// Intentional stop: user cancel, size solved elsewhere, or profile already UNSAT.
    Cancelled,
    /// Unexpected Z3 Unknown — abandon this replica only; siblings may continue.
    Abandoned(String),
    /// Internal solver/model failure that should fail the whole size search.
    Failed(SolveError),
}

/// Run one seeded Z3 attempt, with a watcher thread that interrupts on cancel flags.
#[allow(clippy::too_many_arguments)]
fn run_attempt<F>(
    problem: &Problem,
    node_types: &[OperatorKind],
    node_count: usize,
    profile: usize,
    replica: usize,
    solver_threads: usize,
    cancel: &AtomicBool,
    stop_all: &AtomicBool,
    stop_profile: &AtomicBool,
    progress: &SizeProgressContext<'_, F>,
    enumerate: bool,
    enumerated: &Mutex<Vec<Solution>>,
    seen_identities: &Mutex<HashSet<String>>,
) -> AttemptResult
where
    F: FnMut(SolverEvent) + Send,
{
    let config = Config::new();
    with_z3_config(&config, || {
        let context = Context::thread_local();
        let done = AtomicBool::new(false);
        thread::scope(|scope| {
            let handle = context.handle();
            let done_ref = &done;
            scope.spawn(move || {
                while !done_ref.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed)
                        || stop_all.load(Ordering::Relaxed)
                        || stop_profile.load(Ordering::Relaxed)
                    {
                        handle.interrupt();
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            });
            let seed = attempt_seed(node_count, profile, replica);
            let result = run_attempt_in_context(
                problem,
                node_types,
                solver_threads,
                seed,
                cancel,
                stop_all,
                stop_profile,
                progress,
                enumerate,
                enumerated,
                seen_identities,
            );
            done.store(true, Ordering::Relaxed);
            result
        })
    })
}

/// Z3's reason string for an intentional interrupt (as opposed to a genuine Unknown).
fn interrupted_unknown(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("interrupt") || lower.contains("cancel")
}

/// Encode + check loop for one attempt. SAT models that fail verification are blocked
/// on **this** solver only; sibling replicas keep independent clause sets.
#[allow(clippy::too_many_arguments)]
fn run_attempt_in_context<F>(
    problem: &Problem,
    node_types: &[OperatorKind],
    solver_threads: usize,
    random_seed: u32,
    cancel: &AtomicBool,
    stop_all: &AtomicBool,
    stop_profile: &AtomicBool,
    progress: &SizeProgressContext<'_, F>,
    enumerate: bool,
    enumerated: &Mutex<Vec<Solution>>,
    seen_identities: &Mutex<HashSet<String>>,
) -> AttemptResult
where
    F: FnMut(SolverEvent) + Send,
{
    let cancelled = || {
        cancel.load(Ordering::Relaxed)
            || stop_all.load(Ordering::Relaxed)
            || stop_profile.load(Ordering::Relaxed)
    };
    if cancelled() {
        return AttemptResult::Cancelled;
    }
    let Some(encoding) = Encoding::try_new_with_threads(
        problem,
        node_types,
        solver_threads,
        random_seed,
        cancel,
        stop_all,
        stop_profile,
    ) else {
        return AttemptResult::Cancelled;
    };
    report_size_progress(false, progress);
    loop {
        if cancelled() {
            return AttemptResult::Cancelled;
        }
        match encoding.solver.check() {
            SatResult::Unsat => break,
            SatResult::Unknown => {
                if cancelled() {
                    return AttemptResult::Cancelled;
                }
                let reason = encoding
                    .solver
                    .get_reason_unknown()
                    .unwrap_or_else(|| "Z3 returned unknown".to_owned());
                if interrupted_unknown(&reason) {
                    return AttemptResult::Cancelled;
                }
                return AttemptResult::Abandoned(reason);
            }
            SatResult::Sat => {
                let Some(model) = encoding.solver.get_model() else {
                    return AttemptResult::Failed(SolveError::Solver(
                        "Z3 returned no model".to_owned(),
                    ));
                };
                let candidate = match encoding.extract_candidate(&model) {
                    Ok(candidate) => candidate,
                    Err(error) => return AttemptResult::Failed(error),
                };
                if let Ok(verified) = verify_candidate(problem, candidate.clone()) {
                    if enumerate {
                        let solution = build_solution(problem, &verified);
                        let identity = solution_identity(&solution);
                        let is_new = {
                            let mut seen = seen_identities.lock().expect("seen identities");
                            seen.insert(identity)
                        };
                        if is_new {
                            {
                                let mut solutions = enumerated.lock().expect("enumerated");
                                solutions.push(solution.clone());
                            }
                            (progress.on_progress.lock().expect("progress callback"))(
                                SolverEvent::SolutionFound(Box::new(solution)),
                            );
                        }
                        encoding.block_candidate(&candidate);
                        report_size_progress(false, progress);
                        continue;
                    }
                    return AttemptResult::Found(verified);
                }
                progress.rejected.fetch_add(1, Ordering::Relaxed);
                encoding.block_candidate(&candidate);
                report_size_progress(false, progress);
            }
        }
    }
    AttemptResult::Unsat
}

/// All operator-kind multisets of length `node_count` that can meet denominator and
/// port/discard balance. Each profile is an independent SMT obligation at this size.
fn operator_profiles(problem: &Problem, node_count: usize) -> Vec<Vec<OperatorKind>> {
    let q = required_denominator(problem);
    let discard_min = if problem.discard_rate.is_zero() {
        0
    } else {
        minimum_discard_belts(problem)
    };
    let mut profiles = Vec::new();
    for splitter_2 in 0..=node_count {
        for splitter_3 in 0..=node_count - splitter_2 {
            for merger_2 in 0..=node_count - splitter_2 - splitter_3 {
                let merger_3 = node_count - splitter_2 - splitter_3 - merger_2;
                if !arity_product_covers(splitter_2, splitter_3, &q) {
                    continue;
                }
                let port_delta = isize::try_from(problem.inputs.len()).expect("endpoint limit")
                    - isize::try_from(problem.outputs.len()).expect("endpoint limit")
                    + isize::try_from(splitter_2).expect("node count fits isize")
                    + 2 * isize::try_from(splitter_3).expect("node count fits isize")
                    - isize::try_from(merger_2).expect("node count fits isize")
                    - 2 * isize::try_from(merger_3).expect("node count fits isize");
                let discard_count_matches = if problem.discard_rate.is_zero() {
                    port_delta == 0
                } else {
                    port_delta
                        >= isize::try_from(discard_min).expect("discard belt count fits isize")
                };
                if !discard_count_matches {
                    continue;
                }
                let mut profile = Vec::with_capacity(node_count);
                profile.extend(std::iter::repeat_n(OperatorKind::Splitter2, splitter_2));
                profile.extend(std::iter::repeat_n(OperatorKind::Splitter3, splitter_3));
                profile.extend(std::iter::repeat_n(OperatorKind::Merger2, merger_2));
                profile.extend(std::iter::repeat_n(OperatorKind::Merger3, merger_3));
                profiles.push(profile);
            }
        }
    }
    profiles
}

/// Full-index producers that exist for this profile (external inputs + active node outs).
fn active_producers(input_count: usize, node_types: &[OperatorKind]) -> Vec<usize> {
    let mut producers = (0..input_count).collect::<Vec<_>>();
    for (node, kind) in node_types.iter().copied().enumerate() {
        for port in 0..kind.output_count() {
            producers.push(node_producer(input_count, node, port));
        }
    }
    producers
}

/// Full-index consumers that exist for this profile (active node ins + external outputs).
fn active_consumers(output_count: usize, node_types: &[OperatorKind]) -> Vec<usize> {
    let mut consumers = Vec::new();
    for (node, kind) in node_types.iter().copied().enumerate() {
        for port in 0..kind.input_count() {
            consumers.push(node_consumer(node, port));
        }
    }
    let node_count = node_types.len();
    consumers.extend((0..output_count).map(|output| output_consumer(node_count, output)));
    consumers
}

/// Map full port indexes → compact SMT variable indexes (`None` if inactive).
fn compact_indexes(full_count: usize, active: &[usize]) -> Vec<Option<usize>> {
    let mut indexes = vec![None; full_count];
    for (compact, &full) in active.iter().enumerate() {
        indexes[full] = Some(compact);
    }
    indexes
}

/// Small primes that appear in single-input transfer denominators (capped at 257).
///
/// Empty unless there is exactly one input: multi-input modular witnesses are not
/// activated. When non-empty the solver leaves `QF_LRA` and adds BV residue constraints,
/// which is expensive but dramatically steers the hard cyclic cases.
fn modular_transfer_primes(problem: &Problem) -> Vec<u64> {
    const MAX_MODULAR_PRIME: u64 = 257;
    if problem.inputs.len() != 1 {
        return Vec::new();
    }

    let input_rate = &problem.inputs[0].rate;
    let mut primes = Vec::new();
    for rate in problem
        .outputs
        .iter()
        .map(|endpoint| &endpoint.rate)
        .chain((!problem.discard_rate.is_zero()).then_some(&problem.discard_rate))
    {
        let denominator = (rate / input_rate).denom().clone();
        for prime in (5..=MAX_MODULAR_PRIME).filter(|value| is_prime(*value)) {
            if (&denominator % prime).is_zero() && !primes.contains(&prime) {
                primes.push(prime);
            }
        }
    }
    primes
}

const fn is_prime(value: u64) -> bool {
    if value < 2 {
        return false;
    }
    let mut divisor = 2;
    while divisor * divisor <= value {
        if value.is_multiple_of(divisor) {
            return false;
        }
        divisor += 1;
    }
    true
}

/// One SMT encoding of a fixed operator profile: routing Bools, Real flows, and optional
/// modular BV witness. Built fresh per portfolio attempt (no shared clause state).
struct Encoding {
    solver: Solver,
    node_types: Vec<OperatorKind>,
    edges: Vec<Vec<Bool>>,
    discard: Vec<Bool>,
    producers: Vec<usize>,
    consumers: Vec<usize>,
    input_count: usize,
    node_count: usize,
}

impl Encoding {
    #[cfg(test)]
    fn new_with_threads(
        problem: &Problem,
        fixed_node_types: &[OperatorKind],
        solver_threads: usize,
    ) -> Self {
        Self::try_new_with_threads(
            problem,
            fixed_node_types,
            solver_threads,
            0,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            &AtomicBool::new(false),
        )
        .expect("encoding construction is not cancelled")
    }

    /// Build the full formula. Returns `None` if a cancel flag trips mid-construction.
    ///
    /// `random_seed` is applied to both `sat.random_seed` and `smt.random_seed` so
    /// portfolio replicas explore different search trajectories.
    #[allow(clippy::too_many_lines)]
    fn try_new_with_threads(
        problem: &Problem,
        fixed_node_types: &[OperatorKind],
        solver_threads: usize,
        random_seed: u32,
        cancel: &AtomicBool,
        stop_all: &AtomicBool,
        stop_profile: &AtomicBool,
    ) -> Option<Self> {
        let cancelled = || {
            cancel.load(Ordering::Relaxed)
                || stop_all.load(Ordering::Relaxed)
                || stop_profile.load(Ordering::Relaxed)
        };
        if cancelled() {
            return None;
        }
        let modular_primes = modular_transfer_primes(problem);
        let solver = if modular_primes.is_empty() {
            Solver::new_for_logic("QF_LRA").expect("Z3 supports exact linear arithmetic")
        } else {
            Solver::new()
        };
        let mut parameters = Params::new();
        parameters.set_u32(
            "threads",
            solver_threads.try_into().expect("thread cap fits u32"),
        );
        parameters.set_u32("sat.random_seed", random_seed);
        parameters.set_u32("smt.random_seed", random_seed);
        solver.set_params(&parameters);
        let node_count = fixed_node_types.len();
        let input_count = problem.inputs.len();
        let output_count = problem.outputs.len();
        let full_producer_total = producer_count(input_count, node_count);
        let full_consumer_total = consumer_count(output_count, node_count);
        let zero_real = Real::from_rational(0, 1);
        let belt_rate = Real::from_big_rational(&problem.belt_rate);

        let node_types = fixed_node_types.to_vec();
        let producers = active_producers(input_count, node_types.as_slice());
        let consumers = active_consumers(output_count, node_types.as_slice());
        let producer_index = compact_indexes(full_producer_total, producers.as_slice());
        let consumer_index = compact_indexes(full_consumer_total, consumers.as_slice());
        let producer_total = producers.len();
        let consumer_total = consumers.len();

        let producer_flow = (0..producer_total)
            .map(|producer| Real::new_const(format!("producer_flow_{producer}")))
            .collect::<Vec<_>>();
        let consumer_flow = (0..consumer_total)
            .map(|consumer| Real::new_const(format!("consumer_flow_{consumer}")))
            .collect::<Vec<_>>();
        for flow in &producer_flow {
            solver.assert(flow.gt(&zero_real));
            solver.assert(flow.le(&belt_rate));
        }
        for flow in &consumer_flow {
            solver.assert(flow.gt(&zero_real));
        }
        for (input, endpoint) in problem.inputs.iter().enumerate() {
            let producer = producer_index[input].expect("external input is active");
            solver.assert(producer_flow[producer].eq(Real::from_big_rational(&endpoint.rate)));
        }
        for (output, endpoint) in problem.outputs.iter().enumerate() {
            let consumer = consumer_index[output_consumer(node_count, output)]
                .expect("external output is active");
            solver.assert(consumer_flow[consumer].eq(Real::from_big_rational(&endpoint.rate)));
        }

        let edges = (0..producer_total)
            .map(|producer| {
                (0..consumer_total)
                    .map(|consumer| Bool::new_const(format!("edge_{producer}_{consumer}")))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let discard = (0..producer_total)
            .map(|producer| Bool::new_const(format!("discard_{producer}")))
            .collect::<Vec<_>>();

        let discard_count = producer_total - consumer_total;
        let discard_choices = discard.iter().map(|route| (route, 1)).collect::<Vec<_>>();
        solver.assert(Bool::pb_eq(
            discard_choices.as_slice(),
            i32::try_from(discard_count).expect("port count fits i32"),
        ));

        if problem.discard_rate.is_zero() {
            for route in &discard {
                solver.assert(route.not());
            }
        } else {
            let discarded_flows = discard
                .iter()
                .zip(producer_flow.iter())
                .map(|(route, flow)| route.ite(flow, &zero_real))
                .collect::<Vec<Real>>();
            solver.assert(
                Real::add(discarded_flows.as_slice())
                    .eq(Real::from_big_rational(&problem.discard_rate)),
            );
        }

        for producer in 0..producer_total {
            for consumer in 0..consumer_total {
                let edge = &edges[producer][consumer];
                solver.assert(edge.implies(producer_flow[producer].eq(&consumer_flow[consumer])));
            }
            let mut choices = edges[producer]
                .iter()
                .map(|edge| (edge, 1))
                .collect::<Vec<_>>();
            choices.push((&discard[producer], 1));
            solver.assert(Bool::pb_eq(choices.as_slice(), 1));
        }
        for (consumer, _) in consumer_flow.iter().enumerate() {
            let incoming = (0..producer_total)
                .map(|producer| (&edges[producer][consumer], 1))
                .collect::<Vec<_>>();
            solver.assert(Bool::pb_eq(incoming.as_slice(), 1));
        }

        if cancelled() {
            return None;
        }
        add_port_symmetry_constraints(
            &solver,
            edges.as_slice(),
            discard.as_slice(),
            producer_index.as_slice(),
            consumer_index.as_slice(),
            input_count,
            node_types.as_slice(),
        );
        block_impossible_direct_edges(
            &solver,
            problem,
            edges.as_slice(),
            producer_index.as_slice(),
            consumer_index.as_slice(),
            node_types.as_slice(),
        );
        if cancelled() {
            return None;
        }
        add_reachability_constraints(
            &solver,
            edges.as_slice(),
            discard.as_slice(),
            producers.as_slice(),
            consumers.as_slice(),
            input_count,
            node_types.as_slice(),
        );
        if cancelled() {
            return None;
        }
        add_modular_transfer_constraints(
            &solver,
            edges.as_slice(),
            producers.as_slice(),
            consumers.as_slice(),
            input_count,
            node_types.as_slice(),
            modular_primes.as_slice(),
        );

        for node in 0..node_count {
            let input = |port| {
                &consumer_flow[consumer_index[node_consumer(node, port)]
                    .expect("active operator input has a compact index")]
            };
            let output = |port| {
                &producer_flow[producer_index[node_producer(input_count, node, port)]
                    .expect("active operator output has a compact index")]
            };
            let input_0 = input(0);
            let half = input_0.div(Real::from_rational(2, 1));
            let third = input_0.div(Real::from_rational(3, 1));
            match node_types[node] {
                OperatorKind::Splitter2 => {
                    solver.assert(output(0).eq(&half));
                    solver.assert(output(1).eq(&half));
                }
                OperatorKind::Splitter3 => {
                    solver.assert(output(0).eq(&third));
                    solver.assert(output(1).eq(&third));
                    solver.assert(output(2).eq(&third));
                }
                OperatorKind::Merger2 => {
                    solver.assert(output(0).eq(Real::add(&[input_0, input(1)])));
                }
                OperatorKind::Merger3 => {
                    solver.assert(output(0).eq(Real::add(&[input_0, input(1), input(2)])));
                }
            }
        }

        Some(Self {
            solver,
            node_types,
            edges,
            discard,
            producers,
            consumers,
            input_count,
            node_count,
        })
    }

    fn extract_candidate(&self, model: &Model) -> Result<Candidate, SolveError> {
        let node_types = self.node_types.clone();
        let mut routes = vec![None; producer_count(self.input_count, self.node_count)];
        for (producer, &full_producer) in self.producers.iter().enumerate() {
            if model
                .eval(&self.discard[producer], true)
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                routes[full_producer] = Some(Route::Discard);
                continue;
            }
            let consumer = (0..self.consumers.len()).find(|&consumer| {
                model
                    .eval(&self.edges[producer][consumer], true)
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            });
            let consumer = consumer.ok_or_else(|| {
                SolveError::Solver("active producer has no modeled destination".to_owned())
            })?;
            routes[full_producer] = Some(Route::Consumer(self.consumers[consumer]));
        }
        Ok(Candidate { node_types, routes })
    }

    fn block_candidate(&self, candidate: &Candidate) {
        let mut changes = Vec::new();
        for (producer, &full_producer) in self.producers.iter().enumerate() {
            match &candidate.routes[full_producer] {
                Some(Route::Consumer(consumer)) => {
                    let consumer = self
                        .consumers
                        .iter()
                        .position(|full| full == consumer)
                        .expect("candidate consumer is active in this encoding");
                    changes.push(self.edges[producer][consumer].not());
                }
                Some(Route::Discard) => changes.push(self.discard[producer].not()),
                None => {}
            }
        }
        self.solver.assert(Bool::or(changes.as_slice()));
    }
}

/// Lex-order interchangeable splitter outs / merger ins by compact absolute indexes.
///
/// Ports on the same node are indistinguishable in the game model; without this the
/// solver rediscovers the same topology under port permutations. Absolute indexes are
/// *not* equivariant under same-kind node swaps — that is intentional; we keep this
/// cheap breaker and rely on adjacent same-kind rank ties in reachability instead of
/// full node-lex canonicalization (which regressed hard cases).
fn add_port_symmetry_constraints(
    solver: &Solver,
    edges: &[Vec<Bool>],
    discard: &[Bool],
    producer_index: &[Option<usize>],
    consumer_index: &[Option<usize>],
    input_count: usize,
    node_types: &[OperatorKind],
) {
    let consumer_total = edges.first().map_or(0, Vec::len);
    let producer_total = edges.len();
    for (node, kind) in node_types.iter().copied().enumerate() {
        for left_port in 0..kind.output_count().saturating_sub(1) {
            let right_port = left_port + 1;
            let left = producer_index[node_producer(input_count, node, left_port)]
                .expect("active output has a compact index");
            let right = producer_index[node_producer(input_count, node, right_port)]
                .expect("active output has a compact index");
            for left_target in 0..=consumer_total {
                for right_target in 0..left_target {
                    let left_route = if left_target == consumer_total {
                        &discard[left]
                    } else {
                        &edges[left][left_target]
                    };
                    let right_route = if right_target == consumer_total {
                        &discard[right]
                    } else {
                        &edges[right][right_target]
                    };
                    solver.assert(Bool::and(&[left_route.clone(), right_route.clone()]).not());
                }
            }
        }

        for left_port in 0..kind.input_count().saturating_sub(1) {
            let right_port = left_port + 1;
            let left = consumer_index[node_consumer(node, left_port)]
                .expect("active input has a compact index");
            let right = consumer_index[node_consumer(node, right_port)]
                .expect("active input has a compact index");
            for left_source in 0..producer_total {
                for right_source in 0..=left_source {
                    solver.assert(
                        Bool::and(&[
                            edges[left_source][left].clone(),
                            edges[right_source][right].clone(),
                        ])
                        .not(),
                    );
                }
            }
        }
    }
}

/// Every node must have a path from some input (source rank) and to an output/discard
/// (sink rank), plus a stable tie-break on adjacent identical operator kinds.
///
/// Ranks increase along edges so disconnected circulations are unsat. Same-kind adjacent
/// nodes are ordered by `(source_rank, sink_rank)` to cut labeling symmetry cheaply.
fn add_reachability_constraints(
    solver: &Solver,
    edges: &[Vec<Bool>],
    discard: &[Bool],
    producers: &[usize],
    consumers: &[usize],
    input_count: usize,
    node_types: &[OperatorKind],
) {
    let node_count = node_types.len();
    if node_count == 0 {
        return;
    }

    let lower = Real::from_rational(1, 1);
    let upper = Real::from_rational(i64::try_from(node_count).expect("node count fits i64"), 1);
    let source_rank = (0..node_count)
        .map(|node| Real::new_const(format!("source_rank_{node}")))
        .collect::<Vec<_>>();
    let sink_rank = (0..node_count)
        .map(|node| Real::new_const(format!("sink_rank_{node}")))
        .collect::<Vec<_>>();

    for node in 0..node_count {
        solver.assert(source_rank[node].ge(&lower));
        solver.assert(source_rank[node].le(&upper));
        solver.assert(sink_rank[node].ge(&lower));
        solver.assert(sink_rank[node].le(&upper));

        let mut source_choices = Vec::new();
        for (consumer, &full_consumer) in consumers.iter().enumerate() {
            if full_consumer >= node_count * PORTS || full_consumer / PORTS != node {
                continue;
            }
            for (producer, &full_producer) in producers.iter().enumerate() {
                let edge = &edges[producer][consumer];
                if full_producer < input_count {
                    source_choices.push(edge.clone());
                } else {
                    let source_node = (full_producer - input_count) / PORTS;
                    source_choices.push(Bool::and(&[
                        edge.clone(),
                        source_rank[source_node].lt(&source_rank[node]),
                    ]));
                }
            }
        }
        solver.assert(Bool::or(source_choices.as_slice()));

        let mut sink_choices = Vec::new();
        for (producer, &full_producer) in producers.iter().enumerate() {
            if full_producer < input_count || (full_producer - input_count) / PORTS != node {
                continue;
            }
            sink_choices.push(discard[producer].clone());
            for (consumer, &full_consumer) in consumers.iter().enumerate() {
                let edge = &edges[producer][consumer];
                if full_consumer >= node_count * PORTS {
                    sink_choices.push(edge.clone());
                } else {
                    let target_node = full_consumer / PORTS;
                    sink_choices.push(Bool::and(&[
                        edge.clone(),
                        sink_rank[target_node].lt(&sink_rank[node]),
                    ]));
                }
            }
        }
        solver.assert(Bool::or(sink_choices.as_slice()));
    }

    for node in 1..node_count {
        if node_types[node - 1] != node_types[node] {
            continue;
        }
        solver.assert(source_rank[node - 1].le(&source_rank[node]));
        solver.assert(
            source_rank[node - 1]
                .eq(&source_rank[node])
                .implies(sink_rank[node - 1].le(&sink_rank[node])),
        );
    }
}

/// Modular residue witness for single-input prime-denominator transfers.
///
/// Assigns a residue in `ℤ/p` to each node. External inputs are residue 0. Operator
/// inputs get residues via edge implications (not a dense ite-mux over every producer —
/// that mux form was much slower). Split/merge laws use `bvurem`; a nonzero residue
/// somewhere forces nontrivial modular structure (needed for feedback).
///
/// BV width is chosen so values stay below `4p` without wraparound: with arity ≤ 3 the
/// largest intermediate is `< 3p`.
fn add_modular_transfer_constraints(
    solver: &Solver,
    edges: &[Vec<Bool>],
    producers: &[usize],
    consumers: &[usize],
    input_count: usize,
    node_types: &[OperatorKind],
    primes: &[u64],
) {
    let node_count = node_types.len();
    for &prime in primes {
        if node_count == 0 {
            solver.assert(Bool::from_bool(false));
            continue;
        }

        // Width must hold values strictly below 4p without wraparound: the largest
        // intermediate here is at most 3*(p-1) < 3p < 4p (arity ≤ 3).
        let width = u64::BITS - (4 * prime).leading_zeros();
        debug_assert!(3 * (prime - 1) < 4 * prime);
        let zero = BV::from_u64(0, width);
        let modulus = BV::from_u64(prime, width);
        let residues = (0..node_count)
            .map(|node| BV::new_const(format!("mod_{prime}_node_{node}"), width))
            .collect::<Vec<_>>();
        for residue in &residues {
            solver.assert(residue.bvult(&modulus));
        }

        let producer_residues = producers
            .iter()
            .map(|&producer| {
                if producer < input_count {
                    zero.clone()
                } else {
                    residues[(producer - input_count) / PORTS].clone()
                }
            })
            .collect::<Vec<_>>();

        let input_residues = build_implication_input_residues(
            solver,
            edges,
            consumers,
            node_types,
            producer_residues.as_slice(),
            &modulus,
            prime,
            width,
        );

        for (node, kind) in node_types.iter().copied().enumerate() {
            let input = |port: usize| &input_residues[node][port];
            match kind {
                OperatorKind::Splitter2 | OperatorKind::Splitter3 => {
                    let divisor = BV::from_u64(
                        u64::try_from(kind.output_count()).expect("port count fits u64"),
                        width,
                    );
                    solver.assert(residues[node].bvmul(&divisor).bvurem(&modulus).eq(input(0)));
                }
                OperatorKind::Merger2 | OperatorKind::Merger3 => {
                    let sum = (0..kind.input_count())
                        .fold(zero.clone(), |sum, port| sum.bvadd(input(port)));
                    solver.assert(residues[node].eq(sum.bvurem(&modulus)));
                }
            }
        }

        let nonzero = residues
            .iter()
            .map(|residue| residue.eq(&zero).not())
            .collect::<Vec<_>>();
        solver.assert(Bool::or(nonzero.as_slice()));
    }
}

/// Per operator-input residue vars with `edge(P→C) ⇒ input_residue[C] == producer_residue[P]`.
///
/// Exactly-one producer per consumer (from the routing PB constraints) makes these
/// implications definitional. Skipping self-loop edges here was A/B'd and did not help
/// wall time — Z3 already folds `false ⇒ …` — so every producer is still asserted.
#[allow(clippy::too_many_arguments)]
fn build_implication_input_residues(
    solver: &Solver,
    edges: &[Vec<Bool>],
    consumers: &[usize],
    node_types: &[OperatorKind],
    producer_residues: &[BV],
    modulus: &BV,
    prime: u64,
    width: u32,
) -> Vec<Vec<BV>> {
    let input_residues = node_types
        .iter()
        .enumerate()
        .map(|(node, kind)| {
            (0..kind.input_count())
                .map(|port| BV::new_const(format!("mod_{prime}_in_{node}_{port}"), width))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for ports in &input_residues {
        for residue in ports {
            solver.assert(residue.bvult(modulus));
        }
    }

    let node_consumer_limit = node_types.len() * PORTS;
    for (consumer, &full_consumer) in consumers.iter().enumerate() {
        if full_consumer >= node_consumer_limit {
            continue;
        }
        let node = full_consumer / PORTS;
        let port = full_consumer % PORTS;
        debug_assert!(port < node_types[node].input_count());
        let target = &input_residues[node][port];
        for (producer, residue) in producer_residues.iter().enumerate() {
            solver.assert(edges[producer][consumer].implies(target.eq(residue)));
        }
    }
    input_residues
}

/// Hard-false edges: rate-mismatched direct input→output, and a node's own out→own in.
fn block_impossible_direct_edges(
    solver: &Solver,
    problem: &Problem,
    edges: &[Vec<Bool>],
    producer_index: &[Option<usize>],
    consumer_index: &[Option<usize>],
    node_types: &[OperatorKind],
) {
    let node_count = node_types.len();
    for (input, input_endpoint) in problem.inputs.iter().enumerate() {
        for (output, output_endpoint) in problem.outputs.iter().enumerate() {
            if input_endpoint.rate != output_endpoint.rate {
                let producer = producer_index[input].expect("external input is active");
                let consumer = consumer_index[output_consumer(node_count, output)]
                    .expect("external output is active");
                solver.assert(edges[producer][consumer].not());
            }
        }
    }
    for (node, kind) in node_types.iter().copied().enumerate() {
        for source_port in 0..kind.output_count() {
            let producer = producer_index[node_producer(problem.inputs.len(), node, source_port)]
                .expect("active output has a compact index");
            for target_port in 0..kind.input_count() {
                let consumer = consumer_index[node_consumer(node, target_port)]
                    .expect("active input has a compact index");
                solver.assert(edges[producer][consumer].not());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::AtomicBool, time::Duration};

    use super::*;
    use crate::{EndpointRequest, parse_rate};

    fn endpoint(id: &str, rate: &str) -> EndpointRequest {
        EndpointRequest {
            id: id.to_owned(),
            name: id.to_owned(),
            rate: rate.to_owned(),
        }
    }

    fn search_size_quiet(
        problem: &Problem,
        node_count: usize,
        cancel: &AtomicBool,
    ) -> Result<SizeSearch, SolveError> {
        search_size(
            problem,
            cancel,
            &Mutex::new(|_: SolverEvent| {}),
            &EmitThrottle::new(Duration::from_millis(150)),
            SizeSearchOptions {
                node_count,
                lower_bound: node_count_lower_bound(problem),
                rejected_before: 0,
                enumerate: false,
            },
        )
    }

    #[test]
    fn progress_reports_preparing_then_checking() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let events = Mutex::new(Vec::new());
        let result = solve_exact(&request, &AtomicBool::new(false), |event| {
            events.lock().expect("events").push(event);
        })
        .unwrap();
        assert!(matches!(result, SolveTermination::Completed(_)));
        let events = events.into_inner().expect("events");
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    SolverEvent::Progress(SolverProgress::Preparing { lower_bound: 1 })
                )
            }),
            "expected preparing event, got {events:?}"
        );
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    SolverEvent::Progress(SolverProgress::Checking {
                        node_count: 1,
                        lower_bound: 1,
                        ..
                    })
                )
            }),
            "expected checking event, got {events:?}"
        );
    }

    #[test]
    fn direct_belt_needs_no_node() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "60")],
            outputs: vec![endpoint("output", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.stats.node_count, 0);
    }

    #[test]
    fn one_splitter_makes_two_equal_outputs() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.stats.node_count, 1);
        assert_eq!(solution.stats.splitters, 1);
    }

    #[test]
    fn unequal_inputs_can_merge_then_split() {
        let request = SolveRequest {
            inputs: vec![endpoint("a", "30"), endpoint("b", "90")],
            outputs: vec![endpoint("x", "60"), endpoint("y", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.stats.node_count, 2);
    }

    #[test]
    fn required_denominator_uses_the_reduced_input_grain() {
        let fifth = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "1")],
            outputs: vec![endpoint("output", "1/5")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(required_denominator(&fifth), BigInt::from(5));
        assert!(!is_two_three_smooth(&required_denominator(&fifth)));

        let identity = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("a", "7"), endpoint("b", "7")],
            outputs: vec![endpoint("x", "7"), endpoint("y", "7")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(required_denominator(&identity), BigInt::from(1));

        let two_three = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "216")],
            outputs: vec![endpoint("a", "66"), endpoint("b", "150")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(required_denominator(&two_three), BigInt::from(36));
        assert!(is_two_three_smooth(&required_denominator(&two_three)));

        let prime = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "358")],
            outputs: vec![
                endpoint("a", "144"),
                endpoint("b", "56"),
                endpoint("c", "50"),
                endpoint("d", "108"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(required_denominator(&prime), BigInt::from(179));
        assert!(!is_two_three_smooth(&required_denominator(&prime)));
    }

    #[test]
    fn node_count_lower_bound_combines_denominator_and_branch_constraints() {
        let merge_problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("a", "30"), endpoint("b", "90")],
            outputs: vec![endpoint("output", "120")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&merge_problem), 1);

        let discard_problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("output", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&discard_problem), 1);

        let identity = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("a", "7"), endpoint("b", "7")],
            outputs: vec![endpoint("x", "7"), endpoint("y", "7")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&identity), 0);

        let fifth = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "1")],
            outputs: vec![endpoint("output", "1/5")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&fifth), 3);

        let two_three = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "216")],
            outputs: vec![endpoint("a", "66"), endpoint("b", "150")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&two_three), 7);

        let prime = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "358")],
            outputs: vec![
                endpoint("a", "144"),
                endpoint("b", "56"),
                endpoint("c", "50"),
                endpoint("d", "108"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(node_count_lower_bound(&prime), 9);
        assert!(operator_profiles(&prime, 8).is_empty());
        assert!(!operator_profiles(&prime, 9).is_empty());
    }

    #[test]
    fn detects_small_prime_transfer_denominators() {
        let problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "358")],
            outputs: vec![
                endpoint("a", "144"),
                endpoint("b", "56"),
                endpoint("c", "50"),
                endpoint("d", "108"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        assert_eq!(modular_transfer_primes(&problem), vec![179]);
    }

    #[test]
    fn derives_automatic_supply_from_the_exact_output_sum() {
        let problem = normalize_problem(&SolveRequest {
            inputs: Vec::new(),
            outputs: vec![endpoint("a", "125/3"), endpoint("b", "25/3")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();

        assert_eq!(problem.inputs.len(), 1);
        assert_eq!(problem.inputs[0].name, "Automatic supply");
        assert_eq!(problem.total_input, parse_rate("50").unwrap());
        assert_eq!(problem.total_input, problem.total_output);
        assert!(problem.discard_rate.is_zero());
    }

    #[test]
    fn automatic_supply_uses_multiple_belts_when_the_sum_exceeds_capacity() {
        let problem = normalize_problem(&SolveRequest {
            inputs: Vec::new(),
            outputs: vec![endpoint("a", "800"), endpoint("b", "800")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();

        assert_eq!(problem.inputs.len(), 2);
        assert_eq!(problem.inputs[0].rate, parse_rate("1200").unwrap());
        assert_eq!(problem.inputs[1].rate, parse_rate("400").unwrap());
        assert_eq!(problem.total_input, problem.total_output);
    }

    #[test]
    fn encoding_allocates_only_active_ports() {
        let problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        let encoding = Encoding::new_with_threads(&problem, &[OperatorKind::Splitter2], 1);
        assert_eq!(encoding.edges.len(), 3);
        assert!(encoding.edges.iter().all(|routes| routes.len() == 3));
    }

    #[test]
    fn smt_rejects_a_disconnected_circulation() {
        let problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "60")],
            outputs: vec![endpoint("output", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        let encoding = Encoding::new_with_threads(
            &problem,
            &[OperatorKind::Splitter2, OperatorKind::Merger2],
            1,
        );
        encoding.solver.assert(&encoding.edges[0][3]);
        encoding.solver.assert(&encoding.edges[1][1]);
        encoding.solver.assert(&encoding.edges[2][2]);
        encoding.solver.assert(&encoding.edges[3][0]);
        assert_eq!(encoding.solver.check(), SatResult::Unsat);
    }

    #[test]
    fn surplus_goes_to_the_infinite_sink() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "1200")],
            outputs: vec![endpoint("output", "400")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.discard_rate.exact, "800");
        assert_eq!(solution.stats.node_count, 1);
        assert!(
            solution
                .nodes
                .iter()
                .any(|node| node.kind == crate::NodeKind::Discard)
        );
        assert!(
            solution
                .edges
                .iter()
                .filter(|edge| edge.discarded)
                .all(|edge| {
                    solution
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.target && node.kind == crate::NodeKind::Discard)
                })
        );
    }

    #[test]
    fn discard_rendering_keeps_every_belt_within_capacity() {
        let request = SolveRequest {
            inputs: vec![endpoint("a", "1200"), endpoint("b", "1200")],
            outputs: Vec::new(),
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.stats.node_count, 0);
        assert_eq!(
            solution
                .nodes
                .iter()
                .filter(|node| node.kind == crate::NodeKind::Discard)
                .count(),
            2
        );
        assert!(
            solution
                .edges
                .iter()
                .filter(|edge| edge.discarded)
                .all(|edge| {
                    solution
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.target && node.kind == crate::NodeKind::Discard)
                })
        );
        assert!(solution.edges.iter().all(|edge| {
            parse_rate(&edge.rate.exact).unwrap() <= parse_rate(&solution.belt_rate.exact).unwrap()
        }));
    }

    #[test]
    fn smt_rejects_an_over_capacity_discard_belt() {
        let problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("a", "800"), endpoint("b", "800")],
            outputs: Vec::new(),
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        let encoding = Encoding::new_with_threads(&problem, &[OperatorKind::Merger2], 1);
        assert_eq!(encoding.solver.check(), SatResult::Unsat);
    }

    #[test]
    #[ignore = "manual hard-case performance benchmark"]
    fn benchmarks_prime_denominator_unsat_prefix() {
        let problem = normalize_problem(&SolveRequest {
            inputs: vec![endpoint("input", "358")],
            outputs: vec![
                endpoint("a", "144"),
                endpoint("b", "56"),
                endpoint("c", "50"),
                endpoint("d", "108"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .unwrap();
        let cancel = AtomicBool::new(false);
        let lower = node_count_lower_bound(&problem);
        for count in 0..lower {
            let started = std::time::Instant::now();
            let result = search_size_quiet(&problem, count, &cancel).unwrap();
            eprintln!("count {count}: {:?}", started.elapsed());
            assert!(matches!(result, SizeSearch::Unsatisfiable(_)));
        }
    }

    #[test]
    fn feedback_loop_can_make_an_exact_fifth() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "1")],
            outputs: vec![endpoint("output", "1/5")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let result = solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let SolveTermination::Completed(solution) = result else {
            panic!("solver was cancelled");
        };
        assert_eq!(solution.stats.node_count, 3);
        assert!(solution.stats.feedback_loops >= 1);
        assert_eq!(
            solution.edges.iter().filter(|edge| edge.feedback).count(),
            solution.stats.feedback_loops
        );
        assert!(
            solution
                .edges
                .iter()
                .filter(|edge| edge.feedback)
                .all(|edge| {
                    solution.nodes.iter().any(|node| {
                        node.id == edge.target
                            && matches!(
                                node.kind,
                                crate::NodeKind::Merger2 | crate::NodeKind::Merger3
                            )
                    })
                })
        );
    }

    #[test]
    fn finds_the_seven_node_216_to_66_and_150_layout_quickly() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "216")],
            outputs: vec![endpoint("a", "66"), endpoint("b", "150")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let started = std::time::Instant::now();
        let problem = normalize_problem(&request).unwrap();
        let cancel = AtomicBool::new(false);
        let solution = (1..=7)
            .find_map(
                |count| match search_size_quiet(&problem, count, &cancel).unwrap() {
                    SizeSearch::Found(candidate) => Some(build_solution(&problem, &candidate)),
                    SizeSearch::Unsatisfiable(_) => None,
                    SizeSearch::Cancelled
                    | SizeSearch::Enumerated(_)
                    | SizeSearch::FailedEnumerated { .. }
                    | SizeSearch::CancelledEnumerated(_) => panic!("solver was cancelled"),
                },
            )
            .expect("the known seven-building layout should be found");
        assert_eq!(solution.total_input.exact, "216");
        assert_eq!(solution.total_output.exact, "216");
        assert_eq!(solution.stats.node_count, 7);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the exact search took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_pre_cancelled_search_stops_cleanly() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "1")],
            outputs: vec![endpoint("output", "1/7")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        assert!(matches!(
            solve_exact(&request, &AtomicBool::new(true), |_| {}).unwrap(),
            SolveTermination::Cancelled { .. }
        ));
    }

    #[test]
    fn enumerate_all_at_n_streams_unique_layouts() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: true,
        };
        let found_events = Mutex::new(Vec::new());
        let result = solve_exact(&request, &AtomicBool::new(false), |event| {
            if let SolverEvent::SolutionFound(solution) = event {
                found_events.lock().expect("events").push(*solution);
            }
        })
        .unwrap();
        let SolveTermination::Enumerated(solutions) = result else {
            panic!("expected enumerated termination, got {result:?}");
        };
        assert!(!solutions.is_empty());
        assert_eq!(found_events.lock().expect("events").len(), solutions.len());
        assert!(
            solutions
                .iter()
                .all(|solution| solution.stats.node_count == solutions[0].stats.node_count)
        );
        assert!(solutions.iter().all(|solution| {
            solution.stats.belt_count
                <= solution.edges.iter().filter(|edge| !edge.discarded).count()
        }));
    }

    #[test]
    fn finish_enumerate_prefers_incomplete_over_complete_on_portfolio_error() {
        let solution = {
            let request = SolveRequest {
                inputs: vec![endpoint("input", "120")],
                outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
                belt_rate: "1200".to_owned(),
                enumerate_all_at_n: false,
            };
            let SolveTermination::Completed(solution) =
                solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap()
            else {
                panic!("expected a completed solution");
            };
            *solution
        };
        let stuck = SolveError::Solver("portfolio stuck".to_owned());

        let incomplete =
            finish_enumerate_size(false, vec![solution.clone()], Some(stuck.clone()), 0)
                .expect("partial failure stays Ok");
        assert!(matches!(incomplete, SizeSearch::FailedEnumerated { .. }));

        let empty_err = finish_enumerate_size(false, Vec::new(), Some(stuck), 0);
        assert!(matches!(empty_err, Err(SolveError::Solver(_))));

        let complete = finish_enumerate_size(false, vec![solution], None, 0).unwrap();
        assert!(matches!(complete, SizeSearch::Enumerated(_)));
    }

    #[test]
    fn classic_solution_reports_operator_belt_metrics() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "120")],
            outputs: vec![endpoint("a", "60"), endpoint("b", "60")],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let SolveTermination::Completed(solution) =
            solve_exact(&request, &AtomicBool::new(false), |_| {}).unwrap()
        else {
            panic!("expected a completed solution");
        };
        // Single splitter: only I/O stubs, no operator↔operator belts.
        assert_eq!(solution.stats.belt_count, 0);
        assert_eq!(solution.stats.internal_max_throughput.exact, "0");
    }

    #[test]
    fn an_active_parallel_search_stops_cleanly() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "358")],
            outputs: vec![
                endpoint("a", "144"),
                endpoint("b", "56"),
                endpoint("c", "50"),
                endpoint("d", "108"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let cancel = AtomicBool::new(false);
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(100));
                cancel.store(true, Ordering::Relaxed);
            });
            assert!(matches!(
                solve_exact(&request, &cancel, |_| {}).unwrap(),
                SolveTermination::Cancelled { .. }
            ));
        });
    }

    #[test]
    fn rejects_rates_above_the_normal_belt_limit() {
        let request = SolveRequest {
            inputs: vec![endpoint("input", "1201")],
            outputs: Vec::new(),
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        assert!(matches!(
            solve_exact(&request, &AtomicBool::new(false), |_| {}),
            Err(SolveError::InvalidRequest(_))
        ));
    }

    #[test]
    #[ignore = "manual portfolio threads_per_attempt matrix on hard cyclic cases"]
    fn profile_portfolio_threads_per_attempt_matrix() {
        let available = thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let cases = [
            ("81,42", vec!["81", "42"]),
            ("60,20,108,50", vec!["60", "20", "108", "50"]),
            ("150,45,63", vec!["150", "45", "63"]),
        ];
        let thread_options = [1usize, 2, 4, 8, 16]
            .into_iter()
            .filter(|threads| *threads <= available)
            .collect::<Vec<_>>();
        eprintln!(
            "available_parallelism={available} default@1profile_T={} T_options={thread_options:?}",
            default_threads_per_attempt(available, 1)
        );

        for threads_per_attempt in thread_options {
            let slots = (available / threads_per_attempt).max(1);
            eprintln!("######## T={threads_per_attempt} => attempt_slots={slots} ########");
            for (label, outputs) in &cases {
                let request = SolveRequest {
                    inputs: Vec::new(),
                    outputs: outputs
                        .iter()
                        .enumerate()
                        .map(|(index, rate)| endpoint(&format!("o{index}"), rate))
                        .collect(),
                    belt_rate: "1200".to_owned(),
                    enumerate_all_at_n: false,
                };
                let problem = normalize_problem(&request).unwrap();
                let cancel = AtomicBool::new(false);
                let mut node_count = node_count_lower_bound(&problem);
                let wall_started = std::time::Instant::now();
                eprintln!(
                    "=== {label} T={threads_per_attempt} profiles@lb={} ===",
                    operator_profiles(&problem, node_count).len()
                );
                loop {
                    let size_started = std::time::Instant::now();
                    let profile_list = operator_profiles(&problem, node_count);
                    let profiles = profile_list.len();
                    let result = search_size_with_threads_per_attempt(
                        &problem,
                        node_count,
                        &cancel,
                        threads_per_attempt,
                        Some(profile_list),
                        &Mutex::new(|_: SolverEvent| {}),
                        &EmitThrottle::new(Duration::from_millis(150)),
                        node_count_lower_bound(&problem),
                        0,
                        false,
                    )
                    .unwrap();
                    eprintln!(
                        "size {node_count} profiles={profiles}: wall={:?} outcome={}",
                        size_started.elapsed(),
                        result.label()
                    );
                    match result {
                        SizeSearch::Found(candidate) => {
                            let solution = build_solution(&problem, &candidate);
                            eprintln!(
                                "FOUND nodes={} feedback={} total_wall={:?}",
                                solution.stats.node_count,
                                solution.stats.feedback_loops,
                                wall_started.elapsed()
                            );
                            break;
                        }
                        SizeSearch::Unsatisfiable(_) => node_count += 1,
                        SizeSearch::Cancelled
                        | SizeSearch::Enumerated(_)
                        | SizeSearch::FailedEnumerated { .. }
                        | SizeSearch::CancelledEnumerated(_) => panic!("cancelled"),
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "manual production portfolio policy: 150,45,63 size-8 unsat only"]
    fn profile_production_150_45_63_size8_unsat() {
        let available = thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let request = SolveRequest {
            inputs: Vec::new(),
            outputs: vec![
                endpoint("o0", "150"),
                endpoint("o1", "45"),
                endpoint("o2", "63"),
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        };
        let problem = normalize_problem(&request).unwrap();
        let node_count = 8;
        let profiles = operator_profiles(&problem, node_count);
        let t = default_threads_per_attempt(available, profiles.len().max(1));
        let slots = (available / t).max(1);
        eprintln!(
            "150,45,63 size={node_count} profiles={} T={t} slots={slots} cpus={available}",
            profiles.len()
        );
        for rep in 1..=3 {
            let cancel = AtomicBool::new(false);
            let started = std::time::Instant::now();
            let result = search_size_quiet(&problem, node_count, &cancel).unwrap();
            eprintln!(
                "rep {rep}: wall={:?} outcome={}",
                started.elapsed(),
                result.label()
            );
            assert!(
                matches!(result, SizeSearch::Unsatisfiable(_)),
                "expected unsat at size 8, got {}",
                result.label()
            );
        }
    }
}
