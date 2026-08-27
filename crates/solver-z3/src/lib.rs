//! Z3 implementation of the common exact solver API.
mod model;
mod public;
mod solver;
mod verify;

pub use public::solve_problem;
