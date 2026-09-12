/// An interrupted or invalid leaf never establishes UNSAT.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    #[error("search interrupted")]
    Cancelled,
    #[error("{0}")]
    Worker(String),
}
