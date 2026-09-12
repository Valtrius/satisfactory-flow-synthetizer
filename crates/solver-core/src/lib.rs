//! Portable exact topology planning and leaf execution, with an optional native host.
pub mod leaf;
pub mod lower_bound;
pub mod planner;
pub mod problem;
pub mod profile;
pub use encoding::Counts;
pub use failure::Failure;
pub use problem::{NormalizedProblem, Preparation, prepare_problem};
mod cardinality;
#[cfg(feature = "native-runtime")]
mod diagnostics;
mod encoding;
mod failure;
#[cfg(feature = "native-runtime")]
mod native;
#[cfg(feature = "native-runtime")]
mod native_api;
#[cfg(feature = "native-runtime")]
mod portfolio;
#[cfg(feature = "native-runtime")]
mod process;
mod restore;
#[cfg(feature = "native-runtime")]
use native::run;
#[cfg(feature = "native-runtime")]
pub use native_api::{
    cvc5_executable, enumerate_minimum_links_with_observer, enumerate_with_observer, solve_problem,
    solve_with_observer,
};
