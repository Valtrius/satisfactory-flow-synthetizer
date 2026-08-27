//! Exact component contracts, capacity behavior, and conservative Pareto pruning.
//!
//! Components accelerate search; they never replace canonical physical port
//! search as the completeness path. Every proof routine in this module is
//! deliberately one-sided. A verified certificate may establish containment,
//! transparency, or dominance. Failure to construct one means `Unknown` and
//! must not remove a component or a physical branch.

use std::collections::{BTreeMap, BTreeSet};

use solver_api::{
    ConsumerPortRef, InputTerminalIndex, NodeProfile, NodeType, OutputTerminalIndex, PhysicalGraph,
    PhysicalLink, PhysicalNode, Problem, ProducerPortRef, Rational,
};
use thiserror::Error;

use crate::{
    canonical::{
        PartialLink, PartialTopology, canonical_homogeneous_row_space, canonicalize_marked_link,
        canonicalize_state, canonicalize_witness,
    },
    scc::{
        DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystem, FrozenSubsystemAnalysis,
        FrozenSubsystemDeclaration, RationalMatrix, analyze_frozen_subsystem,
    },
    topology::TopologyState,
};

/// Malformed exact matrix input.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MatrixError {
    /// One row has a different width from the declared column count.
    #[error("matrix row {row} has {actual} columns; expected {expected}")]
    RowWidth {
        /// Zero-based row index.
        row: usize,
        /// Declared column count.
        expected: usize,
        /// Observed row width.
        actual: usize,
    },
    /// A requested row or column ordering is not a permutation of every index.
    #[error("matrix ordering is not a complete permutation")]
    InvalidPermutation,
}

/// Dense row-major exact rational matrix used by component contracts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExactMatrix {
    rows: usize,
    columns: usize,
    entries: Vec<Rational>,
}

impl ExactMatrix {
    /// Constructs a matrix from rows and an explicit column count.
    ///
    /// The explicit count keeps a zero-row matrix's input dimension intact.
    ///
    /// # Errors
    ///
    /// Returns [`MatrixError::RowWidth`] if any row has the wrong width.
    pub fn from_rows(columns: usize, rows: Vec<Vec<Rational>>) -> Result<Self, MatrixError> {
        for (row, values) in rows.iter().enumerate() {
            if values.len() != columns {
                return Err(MatrixError::RowWidth {
                    row,
                    expected: columns,
                    actual: values.len(),
                });
            }
        }
        Ok(Self {
            rows: rows.len(),
            columns,
            entries: rows.into_iter().flatten().collect(),
        })
    }

    /// Returns a zero-row matrix with the requested input dimension.
    #[must_use]
    pub const fn empty(columns: usize) -> Self {
        Self {
            rows: 0,
            columns,
            entries: Vec::new(),
        }
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns.
    #[must_use]
    pub const fn column_count(&self) -> usize {
        self.columns
    }

    /// Borrows one row.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[Rational]> {
        let start = row.checked_mul(self.columns)?;
        let end = start.checked_add(self.columns)?;
        self.entries.get(start..end)
    }

    /// Iterates rows in stored order.
    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[Rational]> {
        (0..self.rows).map(|row| self.row(row).unwrap_or(&[]))
    }

    /// Multiplies by an exact column vector.
    #[must_use]
    pub fn multiply(&self, vector: &[Rational]) -> Option<Vec<Rational>> {
        if vector.len() != self.columns {
            return None;
        }
        Some(
            self.rows()
                .map(|row| {
                    row.iter()
                        .zip(vector)
                        .map(|(coefficient, value)| coefficient * value)
                        .sum()
                })
                .collect(),
        )
    }

    fn from_scc(matrix: &RationalMatrix) -> Self {
        Self {
            rows: matrix.row_count(),
            columns: matrix.column_count(),
            entries: (0..matrix.row_count())
                .flat_map(|row| {
                    matrix
                        .row(row)
                        .expect("SCC matrix rows are complete")
                        .iter()
                        .cloned()
                })
                .collect(),
        }
    }

    fn to_rows(&self) -> Vec<Vec<Rational>> {
        self.rows().map(<[Rational]>::to_vec).collect()
    }

    fn permute_columns(&self, order: &[usize]) -> Result<Self, MatrixError> {
        validate_permutation(order, self.columns)?;
        Self::from_rows(
            self.columns,
            self.rows()
                .map(|row| order.iter().map(|&column| row[column].clone()).collect())
                .collect(),
        )
    }

    fn permute_rows(&self, order: &[usize]) -> Result<Self, MatrixError> {
        validate_permutation(order, self.rows)?;
        Self::from_rows(
            self.columns,
            order
                .iter()
                .map(|&row| self.row(row).expect("permutation is in range").to_vec())
                .collect(),
        )
    }
}

fn validate_permutation(order: &[usize], size: usize) -> Result<(), MatrixError> {
    let values = order.iter().copied().collect::<BTreeSet<_>>();
    if order.len() != size || values.len() != size || values.iter().copied().ne(0..size) {
        return Err(MatrixError::InvalidPermutation);
    }
    Ok(())
}

/// Canonical, topology-ID-free component boundary shape.
///
/// The feedback bitmap records whether a direct link from output `j` to input
/// `i` would connect two ports of the same physical node and is therefore
/// forbidden. Requiring equal signatures during dominance preserves this
/// physical self-link constraint without storing raw node identifiers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundarySignature {
    input_count: usize,
    output_count: usize,
    forbidden_direct_feedback: Vec<bool>,
}

impl BoundarySignature {
    fn from_frozen(frozen: &FrozenSubsystem) -> Self {
        let inputs = &frozen.declaration.boundary_inputs;
        let outputs = &frozen.declaration.boundary_outputs;
        let forbidden_direct_feedback = inputs
            .iter()
            .flat_map(|input| {
                outputs.iter().map(move |output| {
                    consumer_owner(input.port).is_some()
                        && consumer_owner(input.port) == producer_owner(output.port)
                })
            })
            .collect();
        Self {
            input_count: inputs.len(),
            output_count: outputs.len(),
            forbidden_direct_feedback,
        }
    }

    /// Builds an exact physical boundary signature from its row-major
    /// input-by-output direct-feedback relation.
    ///
    /// This constructor is primarily used by persistence and application-work
    /// keys. The relation is physical: a `true` entry means those two boundary
    /// ports belong to the same internal node, so connecting them directly
    /// would create a forbidden self-link.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::BoundarySignatureSize`] when the bitmap does
    /// not have exactly `input_count * output_count` entries or the product
    /// overflows.
    pub fn from_feedback_relation(
        input_count: usize,
        output_count: usize,
        forbidden_direct_feedback: Vec<bool>,
    ) -> Result<Self, ComponentError> {
        let expected =
            input_count
                .checked_mul(output_count)
                .ok_or(ComponentError::BoundarySignatureSize {
                    expected: usize::MAX,
                    actual: forbidden_direct_feedback.len(),
                })?;
        if forbidden_direct_feedback.len() != expected {
            return Err(ComponentError::BoundarySignatureSize {
                expected,
                actual: forbidden_direct_feedback.len(),
            });
        }
        Ok(Self {
            input_count,
            output_count,
            forbidden_direct_feedback,
        })
    }

    /// Returns the number of boundary inputs.
    #[must_use]
    pub const fn input_count(&self) -> usize {
        self.input_count
    }

    /// Returns the number of boundary outputs.
    #[must_use]
    pub const fn output_count(&self) -> usize {
        self.output_count
    }

    /// Borrows the row-major input-by-output self-feedback relation.
    #[must_use]
    pub fn feedback_relation(&self) -> &[bool] {
        &self.forbidden_direct_feedback
    }

    /// Returns whether connecting output `output` directly to input `input`
    /// would violate the physical same-node self-link prohibition.
    #[must_use]
    pub fn forbids_direct_feedback(&self, input: usize, output: usize) -> Option<bool> {
        let index = input.checked_mul(self.output_count)?.checked_add(output)?;
        self.forbidden_direct_feedback.get(index).copied()
    }

    pub(crate) fn permute(&self, inputs: &[usize], outputs: &[usize]) -> Result<Self, MatrixError> {
        validate_permutation(inputs, self.input_count)?;
        validate_permutation(outputs, self.output_count)?;
        let forbidden_direct_feedback = inputs
            .iter()
            .flat_map(|&input| {
                outputs.iter().map(move |&output| {
                    self.forbids_direct_feedback(input, output)
                        .expect("permutations are in range")
                })
            })
            .collect();
        Ok(Self {
            input_count: self.input_count,
            output_count: self.output_count,
            forbidden_direct_feedback,
        })
    }
}

fn consumer_owner(reference: ConsumerPortRef) -> Option<solver_api::NodeId> {
    match reference {
        ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_) => None,
        ConsumerPortRef::Node { node, .. } => Some(node),
    }
}

fn producer_owner(reference: ProducerPortRef) -> Option<solver_api::NodeId> {
    match reference {
        ProducerPortRef::Input(_) => None,
        ProducerPortRef::Node { node, .. } => Some(node),
    }
}

/// Exact homogeneous application domain `R*x=0` and `Q*x>0`.
///
/// `Q` includes each input basis row and every selected producer row from the
/// frozen subsystem. Thus it covers boundary-input, internal-link, and
/// boundary-output positivity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeasibilityDomain {
    equalities: ExactMatrix,
    strict_rows: ExactMatrix,
    proven_empty: bool,
}

impl FeasibilityDomain {
    fn from_frozen(frozen: &FrozenSubsystem) -> Result<Self, ComponentError> {
        let columns = frozen.boundary_domain.column_count();
        let equalities = canonical_rref(ExactMatrix::from_scc(&frozen.boundary_domain));
        let mut strict_rows = identity_rows(columns);
        strict_rows.extend(ExactMatrix::from_scc(&frozen.all_produced_flow_map).to_rows());
        Self::new(equalities, strict_rows)
    }

    /// Builds and canonicalizes an exact homogeneous domain.
    ///
    /// Equality rows become their unique RREF basis. Strict rows are normalized
    /// only by positive scaling, so their inequality direction cannot flip.
    ///
    /// # Errors
    ///
    /// Returns a dimension error when a strict row does not match the equality
    /// matrix's input count.
    pub fn new(
        equalities: ExactMatrix,
        strict_rows: Vec<Vec<Rational>>,
    ) -> Result<Self, ComponentError> {
        let columns = equalities.column_count();
        let mut normalized = Vec::with_capacity(strict_rows.len());
        let mut proven_empty = false;
        for (row, values) in strict_rows.into_iter().enumerate() {
            if values.len() != columns {
                return Err(ComponentError::Matrix(MatrixError::RowWidth {
                    row,
                    expected: columns,
                    actual: values.len(),
                }));
            }
            match normalize_strict_row(values) {
                Some(values) => normalized.push(values),
                None => proven_empty = true,
            }
        }
        normalized.sort();
        normalized.dedup();
        Ok(Self {
            equalities: canonical_rref(equalities),
            strict_rows: ExactMatrix::from_rows(columns, normalized)?,
            proven_empty,
        })
    }

    /// Returns the boundary-input dimension.
    #[must_use]
    pub const fn input_count(&self) -> usize {
        self.equalities.column_count()
    }

    /// Borrows the canonical equality row-space basis.
    #[must_use]
    pub const fn equalities(&self) -> &ExactMatrix {
        &self.equalities
    }

    /// Borrows canonical strict-positivity rows.
    #[must_use]
    pub const fn strict_rows(&self) -> &ExactMatrix {
        &self.strict_rows
    }

    /// Returns whether an explicit `0>0` row proved this domain empty.
    #[must_use]
    pub const fn is_proven_empty(&self) -> bool {
        self.proven_empty
    }

    /// Tests exact domain membership.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::InputCount`] on a dimension mismatch.
    pub fn contains_exact(&self, input: &[Rational]) -> Result<bool, ComponentError> {
        if input.len() != self.input_count() {
            return Err(ComponentError::InputCount {
                expected: self.input_count(),
                actual: input.len(),
            });
        }
        if self.proven_empty {
            return Ok(false);
        }
        let equalities_hold = self
            .equalities
            .multiply(input)
            .ok_or(ComponentError::FrozenDimensionMismatch)?
            .iter()
            .all(Rational::is_zero);
        let strict_rows_hold = self
            .strict_rows
            .multiply(input)
            .ok_or(ComponentError::FrozenDimensionMismatch)?
            .iter()
            .all(Rational::is_positive);
        Ok(equalities_hold && strict_rows_hold)
    }

    fn permute_columns(&self, order: &[usize]) -> Result<Self, ComponentError> {
        let equalities = self.equalities.permute_columns(order)?;
        let strict_rows = self.strict_rows.permute_columns(order)?.to_rows();
        let mut result = Self::new(equalities, strict_rows)?;
        result.proven_empty |= self.proven_empty;
        Ok(result)
    }
}

fn identity_rows(size: usize) -> Vec<Vec<Rational>> {
    (0..size)
        .map(|diagonal| {
            (0..size)
                .map(|column| {
                    if column == diagonal {
                        Rational::one()
                    } else {
                        Rational::zero()
                    }
                })
                .collect()
        })
        .collect()
}

fn normalize_strict_row(mut row: Vec<Rational>) -> Option<Vec<Rational>> {
    let leading = row.iter().find(|value| !value.is_zero())?.abs();
    for value in &mut row {
        *value = value
            .checked_div(&leading)
            .expect("leading value is nonzero");
    }
    Some(row)
}

fn canonical_rref(matrix: ExactMatrix) -> ExactMatrix {
    let columns = matrix.column_count();
    let mut rows = matrix
        .to_rows()
        .into_iter()
        .filter(|row| row.iter().any(|value| !value.is_zero()))
        .collect::<Vec<_>>();
    drop(matrix);
    let mut pivot_row = 0;
    for column in 0..columns {
        let Some(found) = (pivot_row..rows.len()).find(|&row| !rows[row][column].is_zero()) else {
            continue;
        };
        rows.swap(pivot_row, found);
        let pivot = rows[pivot_row][column].clone();
        for value in &mut rows[pivot_row] {
            *value = value.checked_div(&pivot).expect("pivot is nonzero");
        }
        let pivot_values = rows[pivot_row].clone();
        for (row_index, row) in rows.iter_mut().enumerate() {
            if row_index == pivot_row || row[column].is_zero() {
                continue;
            }
            let factor = row[column].clone();
            for (value, pivot_value) in row.iter_mut().zip(&pivot_values) {
                *value = &*value - &(factor.clone() * pivot_value);
            }
        }
        pivot_row += 1;
        if pivot_row == rows.len() {
            break;
        }
    }
    rows.retain(|row| row.iter().any(|value| !value.is_zero()));
    rows.sort();
    ExactMatrix::from_rows(columns, rows).expect("RREF preserves row width")
}

/// Exact conic/row-space identity used to verify one linear implication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearImplicationCertificate {
    /// Nonnegative multipliers for the source domain's strict rows.
    pub strict_multipliers: Vec<Rational>,
    /// Arbitrary multipliers for the source domain's equality rows.
    pub equality_multipliers: Vec<Rational>,
}

impl LinearImplicationCertificate {
    fn verify(
        &self,
        target: &[Rational],
        domain: &FeasibilityDomain,
        require_strict: bool,
    ) -> bool {
        if target.len() != domain.input_count()
            || self.strict_multipliers.len() != domain.strict_rows.row_count()
            || self.equality_multipliers.len() != domain.equalities.row_count()
            || self.strict_multipliers.iter().any(Rational::is_negative)
            || (require_strict && !self.strict_multipliers.iter().any(Rational::is_positive))
        {
            return false;
        }
        let mut reconstructed = vec![Rational::zero(); domain.input_count()];
        accumulate_rows(
            &mut reconstructed,
            &domain.strict_rows,
            &self.strict_multipliers,
        );
        accumulate_rows(
            &mut reconstructed,
            &domain.equalities,
            &self.equality_multipliers,
        );
        reconstructed == target
    }
}

fn accumulate_rows(target: &mut [Rational], rows: &ExactMatrix, multipliers: &[Rational]) {
    for (row, multiplier) in rows.rows().zip(multipliers) {
        for (value, coefficient) in target.iter_mut().zip(row) {
            *value = &*value + &(multiplier * coefficient);
        }
    }
}

/// Exact proof that every point in one domain belongs to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainContainmentProof {
    /// One row-space certificate per superset equality.
    pub equality_proofs: Vec<LinearImplicationCertificate>,
    /// One strict conic certificate per superset positivity row.
    pub strict_proofs: Vec<LinearImplicationCertificate>,
}

/// Exact proof that each left internal-flow row is bounded by one right row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeakDominanceProof {
    /// Per-left-row right-row index and exact nonnegative implication proof.
    pub rows: Vec<PeakRowProof>,
}

/// One row comparison inside [`PeakDominanceProof`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeakRowProof {
    /// Row of the right component used as a pointwise upper bound.
    pub right_row: usize,
    /// Proof that `right_row*x - left_row*x >= 0` on the right domain.
    pub implication: LinearImplicationCertificate,
}

/// Verified universal capacity-transparency proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityTransparencyProof {
    /// Per-internal-row boundary row and nonnegative implication certificate.
    pub rows: Vec<TransparencyRowProof>,
}

/// One internal-flow bound inside [`CapacityTransparencyProof`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransparencyRowProof {
    /// Boundary-rate row used as the upper bound.
    pub boundary_row: usize,
    /// Proof that `boundary_row*x - internal_row*x >= 0` on the domain.
    pub implication: LinearImplicationCertificate,
}

/// Exact status of the universal inequality `h(x) <= b(x)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapacityTransparency {
    /// A verifier accepted a sufficient exact proof.
    Proven(CapacityTransparencyProof),
    /// One exact applicable input proves `h(x) > b(x)`.
    Disproven {
        /// Exact counterexample input.
        input: Vec<Rational>,
        /// Exact internal peak at the counterexample.
        internal_peak: Rational,
        /// Exact boundary peak at the counterexample.
        boundary_peak: Rational,
    },
    /// Neither direction has an exact certificate yet.
    Unknown,
}

/// Exact capacity values for one concrete application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityMetrics {
    /// `h_C(x)`, with zero for a component having no internal links.
    pub internal_peak: Rational,
    /// `b_C(x)`, the largest input or output boundary rate.
    pub boundary_peak: Rational,
    /// Exact ratio `h_C(x)/b_C(x)`.
    pub alpha: Rational,
}

/// Internal and boundary linear maps used for capacity decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityBehavior {
    internal_rows: ExactMatrix,
    boundary_rows: ExactMatrix,
    transparency: CapacityTransparency,
}

impl CapacityBehavior {
    fn new(domain: &FeasibilityDomain, internal_rows: ExactMatrix, transfer: &ExactMatrix) -> Self {
        let mut boundary = identity_rows(domain.input_count());
        boundary.extend(transfer.to_rows());
        let boundary_rows = ExactMatrix::from_rows(domain.input_count(), boundary)
            .expect("identity and transfer dimensions match");
        let transparency = prove_transparency(domain, &internal_rows, &boundary_rows)
            .map_or(CapacityTransparency::Unknown, CapacityTransparency::Proven);
        Self {
            internal_rows,
            boundary_rows,
            transparency,
        }
    }

    /// Borrows exact internal-flow rows used by `h_C`.
    #[must_use]
    pub const fn internal_rows(&self) -> &ExactMatrix {
        &self.internal_rows
    }

    /// Borrows exact input/output boundary rows used by `b_C`.
    #[must_use]
    pub const fn boundary_rows(&self) -> &ExactMatrix {
        &self.boundary_rows
    }

    /// Borrows the current proof status.
    #[must_use]
    pub const fn transparency(&self) -> &CapacityTransparency {
        &self.transparency
    }

    /// Computes `h_C`, `b_C`, and exact `alpha_C`.
    ///
    /// # Errors
    ///
    /// Returns an input-count error or rejects a zero boundary peak, for which
    /// the ratio is undefined.
    pub fn metrics(&self, input: &[Rational]) -> Result<CapacityMetrics, ComponentError> {
        if input.len() != self.boundary_rows.column_count() {
            return Err(ComponentError::InputCount {
                expected: self.boundary_rows.column_count(),
                actual: input.len(),
            });
        }
        let internal_peak = max_or_zero(
            self.internal_rows
                .multiply(input)
                .ok_or(ComponentError::FrozenDimensionMismatch)?,
        );
        let boundary_peak = max_or_zero(
            self.boundary_rows
                .multiply(input)
                .ok_or(ComponentError::FrozenDimensionMismatch)?,
        );
        let Some(alpha) = internal_peak.checked_div(&boundary_peak) else {
            return Err(ComponentError::ZeroBoundaryPeak);
        };
        Ok(CapacityMetrics {
            internal_peak,
            boundary_peak,
            alpha,
        })
    }

    /// Records an exact counterexample if `input` belongs to `domain` and has
    /// `h_C(input) > b_C(input)`.
    ///
    /// A failed probe leaves the existing status unchanged.
    ///
    /// # Errors
    ///
    /// Returns a dimension error from domain or capacity evaluation.
    pub fn record_counterexample(
        &mut self,
        domain: &FeasibilityDomain,
        input: &[Rational],
    ) -> Result<bool, ComponentError> {
        if !domain.contains_exact(input)? {
            return Ok(false);
        }
        let metrics = self.metrics(input)?;
        if metrics.internal_peak <= metrics.boundary_peak {
            return Ok(false);
        }
        self.transparency = CapacityTransparency::Disproven {
            input: input.to_vec(),
            internal_peak: metrics.internal_peak,
            boundary_peak: metrics.boundary_peak,
        };
        Ok(true)
    }
}

fn max_or_zero(values: Vec<Rational>) -> Rational {
    values.into_iter().max().unwrap_or_else(Rational::zero)
}

/// Invalid component construction or exact evaluation request.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentError {
    /// An exact matrix was malformed.
    #[error(transparent)]
    Matrix(#[from] MatrixError),
    /// A physical boundary bitmap did not match its declared dimensions.
    #[error("boundary feedback bitmap has {actual} entries; expected {expected}")]
    BoundarySignatureSize { expected: usize, actual: usize },
    /// A vector has the wrong component input dimension.
    #[error("expected {expected} component inputs, got {actual}")]
    InputCount { expected: usize, actual: usize },
    /// A frozen contract's matrix dimensions disagree.
    #[error("frozen subsystem matrices have inconsistent boundary dimensions")]
    FrozenDimensionMismatch,
    /// The component has no declared input or no declared output.
    #[error("components require at least one input and one output")]
    EmptyBoundary,
    /// A physical count cannot fit its stable public integer representation.
    #[error("component physical count overflow")]
    CountOverflow,
    /// Exact alpha is undefined because the boundary peak is zero.
    #[error("component boundary peak is zero")]
    ZeroBoundaryPeak,
    /// A proof names a row outside its source matrix.
    #[error("component proof row is out of range")]
    ProofRowOutOfRange,
    /// The compact symbolic contract does not span the flattened physical equations.
    #[error("component R/T/K projection is not equivalent to its physical witness")]
    ProjectionEquivalenceMismatch,
    /// A persisted component witness is not a complete sealed physical subsystem.
    #[error("invalid persisted component witness: {0}")]
    InvalidRecordWitness(&'static str),
    /// A persisted derived field disagrees with the independently rebuilt component.
    #[error("persisted component field failed exact reconstruction: {0}")]
    RecordMismatch(&'static str),
}

/// Lexicographic physical implementation cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentCost {
    /// Number of physical splitter/merger nodes.
    pub nodes: u32,
    /// Number of links internal to the component. Boundary links belong to the parent.
    pub internal_links: u32,
}

/// Canonical behavioral bucket used before exact dominance comparisons.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentBehaviorKey {
    boundary: BoundarySignature,
    transfer: ExactMatrix,
}

impl ComponentBehaviorKey {
    /// Borrows the canonical boundary signature.
    #[must_use]
    pub const fn boundary(&self) -> &BoundarySignature {
        &self.boundary
    }

    /// Borrows the canonical transfer matrix.
    #[must_use]
    pub const fn transfer(&self) -> &ExactMatrix {
        &self.transfer
    }
}

/// Canonical mathematical and structural identity used for deterministic order.
///
/// No raw `NodeId`, port reference, link index, insertion ID, or surrounding
/// problem data participates. The projection bytes come from a standalone
/// frozen subgraph with synthetic uniquely colored boundary terminals.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentCanonicalKey {
    boundary: BoundarySignature,
    transfer: ExactMatrix,
    domain: FeasibilityDomain,
    internal_flow_map: ExactMatrix,
    profile: NodeProfile,
    internal_links: u32,
    projection: Vec<u8>,
}

impl ComponentCanonicalKey {
    /// Borrows the canonical physical boundary relation.
    #[must_use]
    pub const fn boundary(&self) -> &BoundarySignature {
        &self.boundary
    }

    /// Borrows the canonical transfer matrix.
    #[must_use]
    pub const fn transfer(&self) -> &ExactMatrix {
        &self.transfer
    }

    /// Borrows the canonical exact feasibility domain.
    #[must_use]
    pub const fn domain(&self) -> &FeasibilityDomain {
        &self.domain
    }

    /// Borrows the canonical internal-link map.
    #[must_use]
    pub const fn internal_flow_map(&self) -> &ExactMatrix {
        &self.internal_flow_map
    }

    /// Returns the physical node inventory encoded by this key.
    #[must_use]
    pub const fn profile(&self) -> NodeProfile {
        self.profile
    }

    /// Returns the number of internal physical links encoded by this key.
    #[must_use]
    pub const fn internal_link_count(&self) -> u32 {
        self.internal_links
    }

    /// Borrows the topology-ID-free canonical projection bytes.
    #[must_use]
    pub fn projection_bytes(&self) -> &[u8] {
        &self.projection
    }
}

/// Mapping between canonical component coordinates and the frozen witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentWitnessMapping {
    /// `canonical input index -> frozen declaration input index`.
    pub input_order: Vec<usize>,
    /// `canonical output index -> frozen declaration output index`.
    pub output_order: Vec<usize>,
    /// `canonical K row -> frozen subsystem internal-link index`.
    pub internal_link_order: Vec<usize>,
}

/// One canonical local physical link with its exact symbolic flow row.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalComponentLink {
    /// Canonically relabeled local producer endpoint.
    pub producer: ProducerPortRef,
    /// Canonically relabeled local consumer endpoint.
    pub consumer: ConsumerPortRef,
    /// Exact `K` row for this physical link.
    pub coefficients: Vec<Rational>,
}

/// Topology-ID-free physical expansion template.
///
/// Node IDs are contiguous local IDs. Macro integration must allocate fresh
/// parent IDs, remap these endpoints in one batch, and attach only the declared
/// boundary endpoints.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalComponentWitness {
    /// Canonically relabeled local physical nodes.
    pub nodes: Vec<PhysicalNode>,
    /// Canonical internal links aligned with component `K` rows.
    pub internal_links: Vec<CanonicalComponentLink>,
    /// Canonical local consumer endpoint for each component input coordinate.
    pub boundary_inputs: Vec<ConsumerPortRef>,
    /// Canonical local producer endpoint for each component output coordinate.
    pub boundary_outputs: Vec<ProducerPortRef>,
}

/// Stable, topology-ID-free component payload suitable for exact persistence.
///
/// Import never trusts these derived fields. It rebuilds the frozen subsystem
/// from [`Self::canonical_witness`], reconstructs a fresh [`Component`], and
/// requires every stored mathematical and canonical field to agree exactly.
/// The private projection-certification token is therefore never serialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRecord {
    /// Canonical behavioral lookup bucket.
    pub behavior_key: ComponentBehaviorKey,
    /// Full deterministic mathematical/physical component identity.
    pub canonical_key: ComponentCanonicalKey,
    /// Physical boundary self-feedback relation.
    pub boundary: BoundarySignature,
    /// Exact output transfer map `T`.
    pub transfer: ExactMatrix,
    /// Exact internal-link map `K`.
    pub internal_flow_map: ExactMatrix,
    /// Exact homogeneous feasibility domain.
    pub domain: FeasibilityDomain,
    /// Intrinsic capacity rows and exact transparency status.
    pub capacity: CapacityBehavior,
    /// Physical node inventory.
    pub profile: NodeProfile,
    /// Number of physical links internal to the frozen subsystem.
    pub internal_links: u32,
    /// Canonical flattened physical subsystem and boundary declaration.
    pub canonical_witness: CanonicalComponentWitness,
}

/// Unforgeable evidence that a component's compact equations were checked
/// against its flattened primitive physical equations during construction.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionCertification;

/// Capacity-independent virtual operator backed by an exact frozen subgraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    behavior_key: ComponentBehaviorKey,
    canonical_key: ComponentCanonicalKey,
    boundary: BoundarySignature,
    transfer: ExactMatrix,
    internal_flow_map: ExactMatrix,
    domain: FeasibilityDomain,
    capacity: CapacityBehavior,
    profile: NodeProfile,
    internal_links: u32,
    frozen_witness: Box<FrozenSubsystem>,
    witness_mapping: ComponentWitnessMapping,
    canonical_witness: CanonicalComponentWitness,
    projection_certification: ProjectionCertification,
}

impl Component {
    /// Builds a canonical component contract from an SCC-certified subsystem.
    ///
    /// Boundary coordinates are jointly canonicalized by exhaustive permutation
    /// over the component's small interface. The minimized tuple includes the
    /// physical feedback signature, `T`, `R`, positivity rows, `K`, and a
    /// standalone canonical structural projection. This keeps raw topology IDs
    /// out of component identity while preserving a mapping for reconstruction.
    ///
    /// # Errors
    ///
    /// Returns an error for inconsistent frozen dimensions, empty boundaries,
    /// physical counts that do not fit public integer types, or a symbolic
    /// projection that is not exactly equivalent to its physical witness.
    pub fn from_frozen(frozen: FrozenSubsystem) -> Result<Self, ComponentError> {
        validate_frozen_dimensions(&frozen)?;
        let profile = profile_of(&frozen)?;
        let internal_links = u32::try_from(frozen.internal_links.len())
            .map_err(|_| ComponentError::CountOverflow)?;
        let signature = BoundarySignature::from_frozen(&frozen);
        let transfer = ExactMatrix::from_scc(&frozen.transfer_map);
        let internal_flow_map = ExactMatrix::from_scc(&frozen.internal_flow_map);
        let domain = FeasibilityDomain::from_frozen(&frozen)?;

        let input_permutations = permutations(signature.input_count());
        let output_permutations = permutations(signature.output_count());
        let mut selected: Option<CanonicalCandidate> = None;
        for input_order in &input_permutations {
            for output_order in &output_permutations {
                let boundary = signature.permute(input_order, output_order)?;
                let oriented_transfer = transfer
                    .permute_rows(output_order)?
                    .permute_columns(input_order)?;
                let oriented_domain = domain.permute_columns(input_order)?;
                let oriented_internal_raw = internal_flow_map.permute_columns(input_order)?;
                let projection_topology =
                    standalone_projection(&frozen, input_order, output_order)?;
                let (oriented_internal, internal_link_order) = order_internal_rows(
                    &projection_topology,
                    &oriented_internal_raw,
                    frozen.internal_links.len(),
                );
                let projection = canonicalize_state(&projection_topology).as_bytes().to_vec();
                let candidate = CanonicalCandidate {
                    score: CanonicalCandidateScore {
                        boundary,
                        transfer: oriented_transfer,
                        domain: oriented_domain,
                        internal_flow_map: oriented_internal,
                        profile,
                        internal_links,
                        projection,
                    },
                    input_order: input_order.clone(),
                    output_order: output_order.clone(),
                    internal_link_order,
                    projection_topology,
                };
                if selected
                    .as_ref()
                    .is_none_or(|current| candidate.score < current.score)
                {
                    selected = Some(candidate);
                }
            }
        }
        let selected = selected.ok_or(ComponentError::EmptyBoundary)?;
        let behavior_key = canonical_behavior_key(&signature, &transfer)?;
        let canonical_key = ComponentCanonicalKey {
            boundary: selected.score.boundary.clone(),
            transfer: selected.score.transfer.clone(),
            domain: selected.score.domain.clone(),
            internal_flow_map: selected.score.internal_flow_map.clone(),
            profile,
            internal_links,
            projection: selected.score.projection,
        };
        let capacity = CapacityBehavior::new(
            &selected.score.domain,
            selected.score.internal_flow_map.clone(),
            &selected.score.transfer,
        );
        let canonical_witness = canonical_component_witness(
            &selected.projection_topology,
            &selected.score.internal_flow_map,
        );
        let projection_certification = certify_projected_equivalence(
            &canonical_witness,
            &selected.score.domain,
            &selected.score.transfer,
            &selected.score.internal_flow_map,
        )?;
        Ok(Self {
            behavior_key,
            canonical_key,
            boundary: selected.score.boundary,
            transfer: selected.score.transfer,
            internal_flow_map: selected.score.internal_flow_map,
            domain: selected.score.domain,
            capacity,
            profile,
            internal_links,
            frozen_witness: Box::new(frozen),
            witness_mapping: ComponentWitnessMapping {
                input_order: selected.input_order,
                output_order: selected.output_order,
                internal_link_order: selected.internal_link_order,
            },
            canonical_witness,
            projection_certification,
        })
    }

    /// Exports the complete canonical, capacity-independent persistence payload.
    ///
    /// Topology-local frozen IDs, mutable cache state, and the private
    /// projection-certification token are deliberately excluded.
    #[must_use]
    pub fn export_record(&self) -> ComponentRecord {
        ComponentRecord {
            behavior_key: self.behavior_key.clone(),
            canonical_key: self.canonical_key.clone(),
            boundary: self.boundary.clone(),
            transfer: self.transfer.clone(),
            internal_flow_map: self.internal_flow_map.clone(),
            domain: self.domain.clone(),
            capacity: self.capacity.clone(),
            profile: self.profile,
            internal_links: self.internal_links,
            canonical_witness: self.canonical_witness.clone(),
        }
    }

    /// Imports a persisted component through a full physical reconstruction.
    ///
    /// The canonical witness is converted back into an ordinary topology with
    /// synthetic boundary terminals. Frozen-subsystem analysis then derives a
    /// fresh `R/T/K` contract, [`Component::from_frozen`] re-runs projection
    /// certification, and this function compares every persisted field with the
    /// independently rebuilt record. A malformed or stale payload can never
    /// manufacture the private certification token.
    ///
    /// # Errors
    ///
    /// Returns a structured record error for malformed physical endpoints,
    /// invalid frozen topology, singular algebra, or any exact field mismatch.
    pub fn import_verified(record: &ComponentRecord) -> Result<Self, ComponentError> {
        let rebuilt = Self::import_canonical_witness(&record.canonical_witness)?;
        let rebuilt_record = rebuilt.export_record();
        compare_component_records(record, &rebuilt_record)?;
        Ok(rebuilt)
    }

    /// Rebuilds a certified component from only its canonical physical witness.
    ///
    /// Persistence decoders use this narrow entry point before comparing their
    /// decoded derived fields with [`Self::export_record`]. The witness is
    /// reconstructed as an ordinary primitive topology, frozen-subsystem
    /// analysis independently derives `R/T/K`, and the private projection token
    /// is issued only after exact equivalence certification. External and
    /// discard endpoints are rejected because a component boundary consists of
    /// declared internal-node ports.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError`] for malformed port coverage, a singular
    /// subsystem, or a failed exact projection certificate.
    pub fn import_canonical_witness(
        witness: &CanonicalComponentWitness,
    ) -> Result<Self, ComponentError> {
        rebuild_component_record_witness(witness)
    }

    /// Borrows the canonical behavioral bucket key.
    #[must_use]
    pub const fn behavior_key(&self) -> &ComponentBehaviorKey {
        &self.behavior_key
    }

    /// Borrows the canonical full component key.
    #[must_use]
    pub const fn canonical_key(&self) -> &ComponentCanonicalKey {
        &self.canonical_key
    }

    /// Borrows the topology-ID-free boundary signature.
    #[must_use]
    pub const fn boundary(&self) -> &BoundarySignature {
        &self.boundary
    }

    /// Borrows exact transfer matrix `T`.
    #[must_use]
    pub const fn transfer(&self) -> &ExactMatrix {
        &self.transfer
    }

    /// Borrows exact internal physical-link map `K`.
    #[must_use]
    pub const fn internal_flow_map(&self) -> &ExactMatrix {
        &self.internal_flow_map
    }

    /// Borrows the exact application domain.
    #[must_use]
    pub const fn domain(&self) -> &FeasibilityDomain {
        &self.domain
    }

    /// Borrows immutable capacity behavior and transparency proof status.
    ///
    /// Search history must not change a certified component: otherwise worker
    /// scheduling and warm/cold lookup order could change later macro choices.
    /// Application-specific counterexample classification operates on a cloned
    /// [`CapacityBehavior`] instead.
    #[must_use]
    pub const fn capacity_behavior(&self) -> &CapacityBehavior {
        &self.capacity
    }

    /// Returns the exact physical node profile consumed by macro expansion.
    #[must_use]
    pub const fn profile(&self) -> NodeProfile {
        self.profile
    }

    /// Returns the internal physical-link count consumed by macro expansion.
    #[must_use]
    pub const fn internal_link_count(&self) -> u32 {
        self.internal_links
    }

    /// Returns lexicographic physical implementation cost.
    #[must_use]
    pub const fn cost(&self) -> ComponentCost {
        ComponentCost {
            nodes: self.profile.node_count(),
            internal_links: self.internal_links,
        }
    }

    /// Borrows the fixed physical witness for later flattening.
    ///
    /// Its raw identifiers are reconstruction data only and never participate
    /// in behavior, dominance, persistence, or frontier order.
    #[must_use]
    pub const fn frozen_witness(&self) -> &FrozenSubsystem {
        &self.frozen_witness
    }

    /// Borrows canonical-to-witness boundary coordinate maps.
    #[must_use]
    pub const fn witness_mapping(&self) -> &ComponentWitnessMapping {
        &self.witness_mapping
    }

    /// Borrows the canonical local physical expansion template.
    #[must_use]
    pub const fn canonical_witness(&self) -> &CanonicalComponentWitness {
        &self.canonical_witness
    }

    /// Returns whether construction certified exact equivalence between the
    /// compact `R/T/K` system and the flattened primitive physical equations.
    ///
    /// Components cannot be created without this private immutable token, so a
    /// macro application may trust this accessor without repeating the full
    /// primitive expansion in its hot loop.
    #[must_use]
    pub const fn is_projection_certified(&self) -> bool {
        let _ = &self.projection_certification;
        true
    }

    /// Returns the frozen consumer endpoint for one canonical input coordinate.
    #[must_use]
    pub fn frozen_input_endpoint(&self, canonical_input: usize) -> Option<ConsumerPortRef> {
        let witness_index = *self.witness_mapping.input_order.get(canonical_input)?;
        self.frozen_witness
            .declaration
            .boundary_inputs
            .get(witness_index)
            .map(|boundary| boundary.port)
    }

    /// Returns the frozen producer endpoint for one canonical output coordinate.
    #[must_use]
    pub fn frozen_output_endpoint(&self, canonical_output: usize) -> Option<ProducerPortRef> {
        let witness_index = *self.witness_mapping.output_order.get(canonical_output)?;
        self.frozen_witness
            .declaration
            .boundary_outputs
            .get(witness_index)
            .map(|boundary| boundary.port)
    }

    /// Evaluates domain, transfer, exact alpha, positivity, and mandatory capacity.
    ///
    /// Capacity remains a hard constraint. Equality with `capacity` is accepted.
    ///
    /// # Errors
    ///
    /// Returns a structured exact application failure.
    pub fn evaluate(
        &self,
        input: &[Rational],
        capacity: &Rational,
    ) -> Result<ComponentApplication, ComponentApplicationError> {
        if input.len() != self.boundary.input_count() {
            return Err(ComponentApplicationError::InputCount {
                expected: self.boundary.input_count(),
                actual: input.len(),
            });
        }
        if !capacity.is_positive() {
            return Err(ComponentApplicationError::NonPositiveCapacity);
        }
        if !self.domain.contains_exact(input)? {
            return Err(ComponentApplicationError::OutsideDomain);
        }
        let outputs = self
            .transfer
            .multiply(input)
            .ok_or(ComponentError::FrozenDimensionMismatch)?;
        let internal_flows = self
            .internal_flow_map
            .multiply(input)
            .ok_or(ComponentError::FrozenDimensionMismatch)?;
        for (index, value) in input.iter().enumerate() {
            if value > capacity {
                return Err(ComponentApplicationError::BoundaryCapacityExceeded {
                    side: BoundarySide::Input,
                    index,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }
        for (index, value) in outputs.iter().enumerate() {
            if value > capacity {
                return Err(ComponentApplicationError::BoundaryCapacityExceeded {
                    side: BoundarySide::Output,
                    index,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }
        for (row, value) in internal_flows.iter().enumerate() {
            if value > capacity {
                return Err(ComponentApplicationError::InternalCapacityExceeded {
                    row,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }
        let capacity_metrics = self.capacity.metrics(input)?;
        Ok(ComponentApplication {
            inputs: input.to_vec(),
            outputs,
            internal_flows,
            capacity: capacity_metrics,
        })
    }

    /// Converts canonical input coordinates to the frozen witness order.
    ///
    /// # Errors
    ///
    /// Returns an input-count error when the vector dimension is wrong.
    pub fn inputs_in_witness_order(
        &self,
        canonical: &[Rational],
    ) -> Result<Vec<Rational>, ComponentError> {
        if canonical.len() != self.witness_mapping.input_order.len() {
            return Err(ComponentError::InputCount {
                expected: self.witness_mapping.input_order.len(),
                actual: canonical.len(),
            });
        }
        let mut witness = vec![Rational::zero(); canonical.len()];
        for (canonical_index, &witness_index) in self.witness_mapping.input_order.iter().enumerate()
        {
            witness[witness_index] = canonical[canonical_index].clone();
        }
        Ok(witness)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalCandidateScore {
    boundary: BoundarySignature,
    transfer: ExactMatrix,
    domain: FeasibilityDomain,
    internal_flow_map: ExactMatrix,
    profile: NodeProfile,
    internal_links: u32,
    projection: Vec<u8>,
}

struct CanonicalCandidate {
    score: CanonicalCandidateScore,
    input_order: Vec<usize>,
    output_order: Vec<usize>,
    internal_link_order: Vec<usize>,
    projection_topology: PartialTopology,
}

/// Boundary side named by an application error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundarySide {
    /// Component input.
    Input,
    /// Component output.
    Output,
}

/// Exact failure to apply a component at concrete boundary rates.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentApplicationError {
    /// The supplied input vector has the wrong dimension.
    #[error("expected {expected} component inputs, got {actual}")]
    InputCount { expected: usize, actual: usize },
    /// Mandatory physical link capacity must be positive.
    #[error("component capacity must be strictly positive")]
    NonPositiveCapacity,
    /// Exact boundary equalities or strict positivity do not hold.
    #[error("component input lies outside its exact feasibility domain")]
    OutsideDomain,
    /// A boundary physical link exceeds capacity.
    #[error("component {side:?} boundary {index} exceeds capacity")]
    BoundaryCapacityExceeded {
        /// Boundary direction.
        side: BoundarySide,
        /// Coordinate index.
        index: usize,
        /// Exact flow.
        value: Box<Rational>,
        /// Exact capacity.
        capacity: Box<Rational>,
    },
    /// An internal physical link exceeds capacity.
    #[error("component internal flow row {row} exceeds capacity")]
    InternalCapacityExceeded {
        /// Internal `K` row.
        row: usize,
        /// Exact flow.
        value: Box<Rational>,
        /// Exact capacity.
        capacity: Box<Rational>,
    },
    /// Internal exact component evaluation contract failed.
    #[error(transparent)]
    Contract(#[from] ComponentError),
}

/// Successful exact component application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentApplication {
    /// Exact canonical-coordinate inputs.
    pub inputs: Vec<Rational>,
    /// Exact canonical-coordinate outputs.
    pub outputs: Vec<Rational>,
    /// Exact internal physical-link flows.
    pub internal_flows: Vec<Rational>,
    /// Exact peak and alpha values.
    pub capacity: CapacityMetrics,
}

fn validate_frozen_dimensions(frozen: &FrozenSubsystem) -> Result<(), ComponentError> {
    let inputs = frozen.declaration.boundary_inputs.len();
    let outputs = frozen.declaration.boundary_outputs.len();
    if inputs == 0 || outputs == 0 {
        return Err(ComponentError::EmptyBoundary);
    }
    if frozen.boundary_domain.column_count() != inputs
        || frozen.all_produced_flow_map.column_count() != inputs
        || frozen.internal_flow_map.column_count() != inputs
        || frozen.transfer_map.column_count() != inputs
        || frozen.transfer_map.row_count() != outputs
        || frozen.internal_flow_map.row_count() != frozen.internal_links.len()
    {
        return Err(ComponentError::FrozenDimensionMismatch);
    }
    Ok(())
}

fn profile_of(frozen: &FrozenSubsystem) -> Result<NodeProfile, ComponentError> {
    let mut profile = NodeProfile::default();
    for node in &frozen.fixed_nodes {
        let count = match node.node_type {
            solver_api::NodeType::Splitter2 => &mut profile.splitter2,
            solver_api::NodeType::Splitter3 => &mut profile.splitter3,
            solver_api::NodeType::Merger2 => &mut profile.merger2,
            solver_api::NodeType::Merger3 => &mut profile.merger3,
        };
        *count = count.checked_add(1).ok_or(ComponentError::CountOverflow)?;
    }
    Ok(profile)
}

fn canonical_behavior_key(
    signature: &BoundarySignature,
    transfer: &ExactMatrix,
) -> Result<ComponentBehaviorKey, ComponentError> {
    let mut result = None;
    for inputs in permutations(signature.input_count()) {
        for outputs in permutations(signature.output_count()) {
            let candidate = ComponentBehaviorKey {
                boundary: signature.permute(&inputs, &outputs)?,
                transfer: transfer.permute_rows(&outputs)?.permute_columns(&inputs)?,
            };
            if result.as_ref().is_none_or(|current| candidate < *current) {
                result = Some(candidate);
            }
        }
    }
    Ok(result.expect("nonempty boundaries have permutations"))
}

fn standalone_projection(
    frozen: &FrozenSubsystem,
    input_order: &[usize],
    output_order: &[usize],
) -> Result<PartialTopology, ComponentError> {
    let input_count = input_order.len();
    let output_count = output_order.len();
    let input_rates = (0..input_count)
        .map(|index| Rational::from(index + 1))
        .collect::<Vec<_>>();
    let output_rates = (0..output_count)
        .map(|index| Rational::from(input_count + index + 1))
        .collect::<Vec<_>>();
    let max_link_rate = Rational::from(input_count + output_count + 1);
    let mut links = frozen
        .internal_links
        .iter()
        .map(|link| PartialLink {
            producer: link.producer,
            consumer: link.consumer,
            flow: None,
        })
        .collect::<Vec<_>>();
    for (terminal, &witness_index) in input_order.iter().enumerate() {
        let terminal = u32::try_from(terminal).map_err(|_| ComponentError::CountOverflow)?;
        links.push(PartialLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(terminal)),
            consumer: frozen.declaration.boundary_inputs[witness_index].port,
            flow: None,
        });
    }
    for (terminal, &witness_index) in output_order.iter().enumerate() {
        let terminal = u32::try_from(terminal).map_err(|_| ComponentError::CountOverflow)?;
        links.push(PartialLink {
            producer: frozen.declaration.boundary_outputs[witness_index].port,
            consumer: ConsumerPortRef::Output(OutputTerminalIndex(terminal)),
            flow: None,
        });
    }
    Ok(PartialTopology {
        problem: Problem {
            inputs: input_rates,
            outputs: output_rates,
            max_link_rate,
        },
        nodes: frozen.fixed_nodes.clone(),
        links,
        discard_count: 0,
        remaining_profile: NodeProfile::default(),
    })
}

fn rebuild_component_record_witness(
    witness: &CanonicalComponentWitness,
) -> Result<Component, ComponentError> {
    if witness.nodes.is_empty()
        || witness.boundary_inputs.is_empty()
        || witness.boundary_outputs.is_empty()
    {
        return Err(ComponentError::InvalidRecordWitness(
            "nodes and both boundary sides must be nonempty",
        ));
    }
    for (index, node) in witness.nodes.iter().enumerate() {
        let expected = u32::try_from(index).map_err(|_| ComponentError::CountOverflow)?;
        if node.id.0 != expected {
            return Err(ComponentError::InvalidRecordWitness(
                "canonical node identifiers must be contiguous",
            ));
        }
    }
    if witness.internal_links.iter().any(|link| {
        !matches!(link.producer, ProducerPortRef::Node { .. })
            || !matches!(link.consumer, ConsumerPortRef::Node { .. })
    }) || witness
        .boundary_inputs
        .iter()
        .any(|port| !matches!(port, ConsumerPortRef::Node { .. }))
        || witness
            .boundary_outputs
            .iter()
            .any(|port| !matches!(port, ProducerPortRef::Node { .. }))
    {
        return Err(ComponentError::InvalidRecordWitness(
            "component endpoints must all belong to internal nodes",
        ));
    }

    let projection = standalone_projection_from_witness(witness)?;
    let topology = TopologyState::from_partial_topology(&projection)
        .map_err(|_| ComponentError::InvalidRecordWitness("physical port topology is malformed"))?;
    let declaration = FrozenSubsystemDeclaration {
        nodes: witness.nodes.iter().map(|node| node.id).collect(),
        boundary_inputs: witness
            .boundary_inputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryInput { port })
            .collect(),
        boundary_outputs: witness
            .boundary_outputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryOutput { port })
            .collect(),
    };
    let analysis = analyze_frozen_subsystem(&topology, &declaration).map_err(|_| {
        ComponentError::InvalidRecordWitness("frozen subsystem declaration is invalid")
    })?;
    let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
        return Err(ComponentError::InvalidRecordWitness(
            "physical subsystem is not uniquely solvable from its boundary",
        ));
    };
    Component::from_frozen(*frozen)
}

fn standalone_projection_from_witness(
    witness: &CanonicalComponentWitness,
) -> Result<PartialTopology, ComponentError> {
    let input_count = witness.boundary_inputs.len();
    let output_count = witness.boundary_outputs.len();
    let mut links = witness
        .internal_links
        .iter()
        .map(|link| PartialLink {
            producer: link.producer,
            consumer: link.consumer,
            flow: None,
        })
        .collect::<Vec<_>>();
    for (index, &consumer) in witness.boundary_inputs.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| ComponentError::CountOverflow)?;
        links.push(PartialLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(index)),
            consumer,
            flow: None,
        });
    }
    for (index, &producer) in witness.boundary_outputs.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| ComponentError::CountOverflow)?;
        links.push(PartialLink {
            producer,
            consumer: ConsumerPortRef::Output(OutputTerminalIndex(index)),
            flow: None,
        });
    }
    Ok(PartialTopology {
        problem: synthetic_boundary_problem(input_count, output_count),
        nodes: witness.nodes.clone(),
        links,
        discard_count: 0,
        remaining_profile: NodeProfile::default(),
    })
}

fn synthetic_boundary_problem(input_count: usize, output_count: usize) -> Problem {
    Problem {
        inputs: (0..input_count)
            .map(|index| Rational::from(index + 1))
            .collect(),
        outputs: (0..output_count)
            .map(|index| Rational::from(input_count + index + 1))
            .collect(),
        max_link_rate: Rational::from(input_count + output_count + 1),
    }
}

fn compare_component_records(
    stored: &ComponentRecord,
    rebuilt: &ComponentRecord,
) -> Result<(), ComponentError> {
    macro_rules! compare {
        ($field:ident) => {
            if stored.$field != rebuilt.$field {
                return Err(ComponentError::RecordMismatch(stringify!($field)));
            }
        };
    }
    compare!(behavior_key);
    compare!(canonical_key);
    compare!(boundary);
    compare!(transfer);
    compare!(internal_flow_map);
    compare!(domain);
    compare!(capacity);
    compare!(profile);
    compare!(internal_links);
    compare!(canonical_witness);
    Ok(())
}

fn order_internal_rows(
    projection: &PartialTopology,
    internal: &ExactMatrix,
    internal_link_count: usize,
) -> (ExactMatrix, Vec<usize>) {
    let mut rows = (0..internal_link_count)
        .map(|index| {
            (
                canonicalize_marked_link(projection, index),
                internal
                    .row(index)
                    .expect("frozen K row count was validated")
                    .to_vec(),
                index,
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let order = rows.iter().map(|row| row.2).collect::<Vec<_>>();
    let matrix = ExactMatrix::from_rows(
        internal.column_count(),
        rows.into_iter().map(|row| row.1).collect(),
    )
    .expect("ordered rows preserve width");
    (matrix, order)
}

fn canonical_component_witness(
    projection: &PartialTopology,
    internal_flow_map: &ExactMatrix,
) -> CanonicalComponentWitness {
    let graph = PhysicalGraph {
        nodes: projection.nodes.clone(),
        links: projection
            .links
            .iter()
            .map(|link| PhysicalLink {
                producer: link.producer,
                consumer: link.consumer,
                flow: Rational::zero(),
            })
            .collect(),
    };
    let canonical = canonicalize_witness(&projection.problem, &graph).graph;
    let canonical_partial = PartialTopology {
        problem: projection.problem.clone(),
        nodes: canonical.nodes.clone(),
        links: canonical
            .links
            .iter()
            .map(|link| PartialLink {
                producer: link.producer,
                consumer: link.consumer,
                flow: None,
            })
            .collect(),
        discard_count: 0,
        remaining_profile: NodeProfile::default(),
    };

    let mut boundary_inputs = vec![None; projection.problem.inputs.len()];
    let mut boundary_outputs = vec![None; projection.problem.outputs.len()];
    let mut internal_endpoints = Vec::new();
    for (index, link) in canonical_partial.links.iter().enumerate() {
        match (link.producer, link.consumer) {
            (ProducerPortRef::Input(terminal), consumer) => {
                boundary_inputs[terminal.0 as usize] = Some(consumer);
            }
            (producer, ConsumerPortRef::Output(terminal)) => {
                boundary_outputs[terminal.0 as usize] = Some(producer);
            }
            (producer @ ProducerPortRef::Node { .. }, consumer @ ConsumerPortRef::Node { .. }) => {
                internal_endpoints.push((
                    canonicalize_marked_link(&canonical_partial, index),
                    producer,
                    consumer,
                ));
            }
            (_, ConsumerPortRef::Discard(_)) => {
                unreachable!("standalone component projections never contain discard terminals")
            }
        }
    }
    internal_endpoints.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let internal_links = internal_endpoints
        .into_iter()
        .zip(internal_flow_map.rows())
        .map(
            |((_key, producer, consumer), coefficients)| CanonicalComponentLink {
                producer,
                consumer,
                coefficients: coefficients.to_vec(),
            },
        )
        .collect();
    CanonicalComponentWitness {
        nodes: canonical.nodes,
        internal_links,
        boundary_inputs: boundary_inputs
            .into_iter()
            .map(|endpoint| endpoint.expect("every synthetic input has one stub"))
            .collect(),
        boundary_outputs: boundary_outputs
            .into_iter()
            .map(|endpoint| endpoint.expect("every synthetic output has one stub"))
            .collect(),
    }
}

/// Proves once that the compact component equations and the flattened
/// primitive operator/link equations have the same rational solution space.
///
/// Equality of exact RREF row spaces is bidirectional implication: neither the
/// projection nor the physical witness admits a flow assignment rejected by
/// the other. The strict domain is separately rebuilt from boundary inputs plus
/// every `T`/`K` producer flow, proving it covers exactly the flattened physical
/// positivity obligations. The private return type prevents later macro code
/// from claiming certification without passing this construction-time check.
fn certify_projected_equivalence(
    witness: &CanonicalComponentWitness,
    domain: &FeasibilityDomain,
    transfer: &ExactMatrix,
    internal_flow_map: &ExactMatrix,
) -> Result<ProjectionCertification, ComponentError> {
    let mut expected_strict_rows = identity_rows(domain.input_count());
    expected_strict_rows.extend(transfer.to_rows());
    expected_strict_rows.extend(internal_flow_map.to_rows());
    let expected_domain = FeasibilityDomain::new(domain.equalities.clone(), expected_strict_rows)?;
    if domain != &expected_domain {
        return Err(ComponentError::ProjectionEquivalenceMismatch);
    }

    let coordinates = projection_coordinates(witness);
    let primitive_rows = flattened_primitive_rows(witness, &coordinates);
    let projected_rows =
        projected_contract_rows(witness, domain, transfer, internal_flow_map, &coordinates);
    let primitive = canonical_homogeneous_row_space(coordinates.variable_count, primitive_rows)
        .ok_or(ComponentError::ProjectionEquivalenceMismatch)?;
    let projected = canonical_homogeneous_row_space(coordinates.variable_count, projected_rows)
        .ok_or(ComponentError::ProjectionEquivalenceMismatch)?;
    if primitive != projected {
        return Err(ComponentError::ProjectionEquivalenceMismatch);
    }
    Ok(ProjectionCertification)
}

struct ProjectionCoordinates {
    producers: BTreeMap<ProducerPortRef, usize>,
    consumers: BTreeMap<ConsumerPortRef, usize>,
    variable_count: usize,
}

fn projection_coordinates(witness: &CanonicalComponentWitness) -> ProjectionCoordinates {
    let mut producer_indices = BTreeMap::new();
    let mut consumer_indices = BTreeMap::new();
    let mut variable_count = 0;
    for node in &witness.nodes {
        for port in 0..node.node_type.output_port_count() {
            producer_indices.insert(
                ProducerPortRef::Node {
                    node: node.id,
                    port,
                },
                variable_count,
            );
            variable_count += 1;
        }
    }
    for node in &witness.nodes {
        for port in 0..node.node_type.input_port_count() {
            consumer_indices.insert(
                ConsumerPortRef::Node {
                    node: node.id,
                    port,
                },
                variable_count,
            );
            variable_count += 1;
        }
    }
    ProjectionCoordinates {
        producers: producer_indices,
        consumers: consumer_indices,
        variable_count,
    }
}

fn flattened_primitive_rows(
    witness: &CanonicalComponentWitness,
    coordinates: &ProjectionCoordinates,
) -> Vec<Vec<Rational>> {
    let mut primitive_rows = Vec::new();
    for node in &witness.nodes {
        let producers = (0..node.node_type.output_port_count())
            .map(|port| {
                coordinates.producers[&ProducerPortRef::Node {
                    node: node.id,
                    port,
                }]
            })
            .collect::<Vec<_>>();
        let consumers = (0..node.node_type.input_port_count())
            .map(|port| {
                coordinates.consumers[&ConsumerPortRef::Node {
                    node: node.id,
                    port,
                }]
            })
            .collect::<Vec<_>>();
        if matches!(node.node_type, NodeType::Splitter2 | NodeType::Splitter3) {
            for &output in &producers[1..] {
                primitive_rows.push(projection_difference_row(
                    coordinates.variable_count,
                    output,
                    producers[0],
                ));
            }
        }
        primitive_rows.push(projection_conservation_row(
            coordinates.variable_count,
            &consumers,
            &producers,
        ));
    }
    for link in &witness.internal_links {
        primitive_rows.push(projection_difference_row(
            coordinates.variable_count,
            coordinates.producers[&link.producer],
            coordinates.consumers[&link.consumer],
        ));
    }
    primitive_rows
}

fn projected_contract_rows(
    witness: &CanonicalComponentWitness,
    domain: &FeasibilityDomain,
    transfer: &ExactMatrix,
    internal_flow_map: &ExactMatrix,
    coordinates: &ProjectionCoordinates,
) -> Vec<Vec<Rational>> {
    let input_indices = witness
        .boundary_inputs
        .iter()
        .map(|port| coordinates.consumers[port])
        .collect::<Vec<_>>();
    let mut projected_rows = Vec::new();
    for coefficients in domain.equalities().rows() {
        projected_rows.push(projection_linear_combination_row(
            coordinates.variable_count,
            &input_indices,
            coefficients,
        ));
    }
    for (&output, coefficients) in witness
        .boundary_outputs
        .iter()
        .map(|port| &coordinates.producers[port])
        .zip(transfer.rows())
    {
        projected_rows.push(projection_expression_row(
            coordinates.variable_count,
            output,
            &input_indices,
            coefficients,
        ));
    }
    for (link, coefficients) in witness.internal_links.iter().zip(internal_flow_map.rows()) {
        projected_rows.push(projection_expression_row(
            coordinates.variable_count,
            coordinates.producers[&link.producer],
            &input_indices,
            coefficients,
        ));
        projected_rows.push(projection_expression_row(
            coordinates.variable_count,
            coordinates.consumers[&link.consumer],
            &input_indices,
            coefficients,
        ));
    }
    projected_rows
}

fn projection_difference_row(variable_count: usize, left: usize, right: usize) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count];
    row[left] = Rational::one();
    row[right] = Rational::from(-1);
    row
}

fn projection_conservation_row(
    variable_count: usize,
    consumers: &[usize],
    producers: &[usize],
) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count];
    for &consumer in consumers {
        row[consumer] = &row[consumer] + Rational::one();
    }
    for &producer in producers {
        row[producer] = &row[producer] - Rational::one();
    }
    row
}

fn projection_linear_combination_row(
    variable_count: usize,
    variables: &[usize],
    coefficients: &[Rational],
) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count];
    for (&variable, coefficient) in variables.iter().zip(coefficients) {
        row[variable] = &row[variable] + coefficient;
    }
    row
}

fn projection_expression_row(
    variable_count: usize,
    target: usize,
    inputs: &[usize],
    coefficients: &[Rational],
) -> Vec<Rational> {
    let mut row = projection_linear_combination_row(variable_count, inputs, coefficients);
    for value in &mut row {
        *value = -&*value;
    }
    row[target] = &row[target] + Rational::one();
    row
}

fn permutations(size: usize) -> Vec<Vec<usize>> {
    fn extend(
        size: usize,
        prefix: &mut Vec<usize>,
        used: &mut [bool],
        output: &mut Vec<Vec<usize>>,
    ) {
        if prefix.len() == size {
            output.push(prefix.clone());
            return;
        }
        for value in 0..size {
            if used[value] {
                continue;
            }
            used[value] = true;
            prefix.push(value);
            extend(size, prefix, used, output);
            prefix.pop();
            used[value] = false;
        }
    }
    let mut output = Vec::new();
    extend(size, &mut Vec::new(), &mut vec![false; size], &mut output);
    output
}

fn prove_transparency(
    domain: &FeasibilityDomain,
    internal: &ExactMatrix,
    boundary: &ExactMatrix,
) -> Option<CapacityTransparencyProof> {
    let mut rows = Vec::with_capacity(internal.row_count());
    for internal_row in internal.rows() {
        let mut proof = None;
        for (boundary_row, boundary_values) in boundary.rows().enumerate() {
            let difference = subtract_rows(boundary_values, internal_row);
            if let Some(implication) = prove_consequence(&difference, domain, false) {
                proof = Some(TransparencyRowProof {
                    boundary_row,
                    implication,
                });
                break;
            }
        }
        rows.push(proof?);
    }
    let proof = CapacityTransparencyProof { rows };
    verify_transparency_proof(&proof, domain, internal, boundary).then_some(proof)
}

/// Verifies a capacity-transparency certificate using exact arithmetic.
///
/// For each internal row `k`, the certificate selects a boundary row `g` and
/// proves `g-k >= 0` throughout the domain. Hence `k*x <= g*x <= b(x)`, and
/// taking the maximum proves `h(x) <= b(x)`.
#[must_use]
pub fn verify_transparency_proof(
    proof: &CapacityTransparencyProof,
    domain: &FeasibilityDomain,
    internal: &ExactMatrix,
    boundary: &ExactMatrix,
) -> bool {
    if internal.column_count() != domain.input_count()
        || boundary.column_count() != domain.input_count()
        || proof.rows.len() != internal.row_count()
    {
        return false;
    }
    proof.rows.iter().enumerate().all(|(row, row_proof)| {
        let Some(boundary_row) = boundary.row(row_proof.boundary_row) else {
            return false;
        };
        let Some(internal_row) = internal.row(row) else {
            return false;
        };
        let target = subtract_rows(boundary_row, internal_row);
        row_proof.implication.verify(&target, domain, false)
    })
}

/// Attempts a sound exact proof that every point of `subset` belongs to `superset`.
///
/// The conservative prover recognizes equality row-space implications and
/// strict rows that are a positive multiple of one source strict row modulo
/// source equalities. More general valid containments return `None`.
#[must_use]
pub fn prove_domain_containment(
    superset: &FeasibilityDomain,
    subset: &FeasibilityDomain,
) -> Option<DomainContainmentProof> {
    if superset.input_count() != subset.input_count() {
        return None;
    }
    if subset.is_proven_empty() {
        // A zero strict row is an exact empty-domain certificate, so containment
        // is vacuous.
        return Some(DomainContainmentProof {
            equality_proofs: Vec::new(),
            strict_proofs: Vec::new(),
        });
    }
    if superset.is_proven_empty() {
        // The subset has no explicit empty-domain certificate. It could still
        // be empty for a harder reason, but treating that as nonempty here is
        // the conservative direction. The zero strict row is represented by
        // `proven_empty` rather than retained in `strict_rows`, so omitting this
        // guard could incorrectly certify an empty superset from empty row
        // lists and eliminate an applicable alternative.
        return None;
    }
    let equality_proofs = superset
        .equalities
        .rows()
        .map(|target| prove_equality(target, subset))
        .collect::<Option<Vec<_>>>()?;
    let strict_proofs = superset
        .strict_rows
        .rows()
        .map(|target| prove_consequence(target, subset, true))
        .collect::<Option<Vec<_>>>()?;
    let proof = DomainContainmentProof {
        equality_proofs,
        strict_proofs,
    };
    verify_domain_containment(&proof, superset, subset).then_some(proof)
}

/// Verifies `subset ⊆ superset` from exact row-space/conic identities.
#[must_use]
pub fn verify_domain_containment(
    proof: &DomainContainmentProof,
    superset: &FeasibilityDomain,
    subset: &FeasibilityDomain,
) -> bool {
    if superset.input_count() != subset.input_count() {
        return false;
    }
    if subset.is_proven_empty()
        && proof.equality_proofs.is_empty()
        && proof.strict_proofs.is_empty()
    {
        return true;
    }
    if superset.is_proven_empty() {
        return false;
    }
    proof.equality_proofs.len() == superset.equalities.row_count()
        && proof.strict_proofs.len() == superset.strict_rows.row_count()
        && proof
            .equality_proofs
            .iter()
            .zip(superset.equalities.rows())
            .all(|(certificate, target)| certificate.verify(target, subset, false))
        && proof
            .strict_proofs
            .iter()
            .zip(superset.strict_rows.rows())
            .all(|(certificate, target)| certificate.verify(target, subset, true))
}

fn prove_peak_dominance(
    left: &ExactMatrix,
    right: &ExactMatrix,
    right_domain: &FeasibilityDomain,
) -> Option<PeakDominanceProof> {
    if left.column_count() != right_domain.input_count()
        || right.column_count() != right_domain.input_count()
    {
        return None;
    }
    if left.row_count() == 0 {
        return Some(PeakDominanceProof { rows: Vec::new() });
    }
    if right.row_count() == 0 {
        return None;
    }
    let mut rows = Vec::with_capacity(left.row_count());
    for left_row in left.rows() {
        let mut proof = None;
        for (right_row, right_values) in right.rows().enumerate() {
            let difference = subtract_rows(right_values, left_row);
            if let Some(implication) = prove_consequence(&difference, right_domain, false) {
                proof = Some(PeakRowProof {
                    right_row,
                    implication,
                });
                break;
            }
        }
        rows.push(proof?);
    }
    let proof = PeakDominanceProof { rows };
    verify_peak_dominance(&proof, left, right, right_domain).then_some(proof)
}

/// Verifies `max(K_left*x) <= max(K_right*x)` throughout `right_domain`.
#[must_use]
pub fn verify_peak_dominance(
    proof: &PeakDominanceProof,
    left: &ExactMatrix,
    right: &ExactMatrix,
    right_domain: &FeasibilityDomain,
) -> bool {
    if left.column_count() != right_domain.input_count()
        || right.column_count() != right_domain.input_count()
        || proof.rows.len() != left.row_count()
    {
        return false;
    }
    proof.rows.iter().enumerate().all(|(left_row, row_proof)| {
        let Some(right_row) = right.row(row_proof.right_row) else {
            return false;
        };
        let Some(left_row) = left.row(left_row) else {
            return false;
        };
        let target = subtract_rows(right_row, left_row);
        row_proof.implication.verify(&target, right_domain, false)
    })
}

fn prove_equality(
    target: &[Rational],
    domain: &FeasibilityDomain,
) -> Option<LinearImplicationCertificate> {
    let equality_multipliers = combination_coefficients(&domain.equalities, target)?;
    Some(LinearImplicationCertificate {
        strict_multipliers: vec![Rational::zero(); domain.strict_rows.row_count()],
        equality_multipliers,
    })
}

fn prove_consequence(
    target: &[Rational],
    domain: &FeasibilityDomain,
    require_strict: bool,
) -> Option<LinearImplicationCertificate> {
    if !require_strict && let Some(certificate) = prove_equality(target, domain) {
        return Some(certificate);
    }
    let target_modulo_equalities = reduce_modulo_equalities(target, &domain.equalities);
    for (strict_row, source) in domain.strict_rows.rows().enumerate() {
        let source_modulo_equalities = reduce_modulo_equalities(source, &domain.equalities);
        let Some(factor) =
            positive_proportional_factor(&target_modulo_equalities, &source_modulo_equalities)
        else {
            continue;
        };
        let difference = target
            .iter()
            .zip(source)
            .map(|(target, source)| target - &(factor.clone() * source))
            .collect::<Vec<_>>();
        let Some(equality_multipliers) = combination_coefficients(&domain.equalities, &difference)
        else {
            continue;
        };
        let mut strict_multipliers = vec![Rational::zero(); domain.strict_rows.row_count()];
        strict_multipliers[strict_row] = factor;
        return Some(LinearImplicationCertificate {
            strict_multipliers,
            equality_multipliers,
        });
    }
    None
}

fn reduce_modulo_equalities(target: &[Rational], equalities: &ExactMatrix) -> Vec<Rational> {
    let mut reduced = target.to_vec();
    for equality in equalities.rows() {
        let Some(pivot) = equality.iter().position(|value| !value.is_zero()) else {
            continue;
        };
        let factor = reduced[pivot].clone();
        if factor.is_zero() {
            continue;
        }
        for (value, coefficient) in reduced.iter_mut().zip(equality) {
            *value = &*value - &(factor.clone() * coefficient);
        }
    }
    reduced
}

fn positive_proportional_factor(left: &[Rational], right: &[Rational]) -> Option<Rational> {
    let pivot = right.iter().position(|value| !value.is_zero())?;
    let factor = left[pivot].checked_div(&right[pivot])?;
    if !factor.is_positive()
        || left
            .iter()
            .zip(right)
            .any(|(left, right)| left != &(factor.clone() * right))
    {
        return None;
    }
    Some(factor)
}

fn combination_coefficients(basis: &ExactMatrix, target: &[Rational]) -> Option<Vec<Rational>> {
    if target.len() != basis.column_count() {
        return None;
    }
    if basis.row_count() == 0 {
        return target.iter().all(Rational::is_zero).then(Vec::new);
    }
    let unknowns = basis.row_count();
    let mut equations = (0..basis.column_count())
        .map(|column| {
            let mut row = basis
                .rows()
                .map(|basis_row| basis_row[column].clone())
                .collect::<Vec<_>>();
            row.push(target[column].clone());
            row
        })
        .collect::<Vec<_>>();
    let mut pivot_row = 0;
    let mut pivots = Vec::new();
    for variable in 0..unknowns {
        let Some(found) =
            (pivot_row..equations.len()).find(|&row| !equations[row][variable].is_zero())
        else {
            continue;
        };
        equations.swap(pivot_row, found);
        let pivot = equations[pivot_row][variable].clone();
        for value in &mut equations[pivot_row] {
            *value = value.checked_div(&pivot).expect("pivot is nonzero");
        }
        let pivot_values = equations[pivot_row].clone();
        for (row_index, row) in equations.iter_mut().enumerate() {
            if row_index == pivot_row || row[variable].is_zero() {
                continue;
            }
            let factor = row[variable].clone();
            for (value, pivot_value) in row.iter_mut().zip(&pivot_values) {
                *value = &*value - &(factor.clone() * pivot_value);
            }
        }
        pivots.push((pivot_row, variable));
        pivot_row += 1;
        if pivot_row == equations.len() {
            break;
        }
    }
    if equations
        .iter()
        .any(|row| row[..unknowns].iter().all(Rational::is_zero) && !row[unknowns].is_zero())
    {
        return None;
    }
    let mut solution = vec![Rational::zero(); unknowns];
    for (row, variable) in pivots {
        solution[variable] = equations[row][unknowns].clone();
    }
    let reconstructed = (0..basis.column_count())
        .map(|column| {
            basis
                .rows()
                .zip(&solution)
                .map(|(row, multiplier)| &row[column] * multiplier)
                .sum::<Rational>()
        })
        .collect::<Vec<_>>();
    (reconstructed == target).then_some(solution)
}

fn subtract_rows(left: &[Rational], right: &[Rational]) -> Vec<Rational> {
    left.iter()
        .zip(right)
        .map(|(left, right)| left - right)
        .collect()
}

/// Exact proof that one component dominates another under one boundary bijection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentDominanceProof {
    /// Reorders the dominating component's inputs into dominated coordinates.
    pub dominating_input_order: Vec<usize>,
    /// Reorders the dominating component's outputs into dominated coordinates.
    pub dominating_output_order: Vec<usize>,
    /// Proof that the dominated domain is contained in the dominating domain.
    pub domain: DomainContainmentProof,
    /// Proof that the dominating internal peak never exceeds the dominated peak.
    pub peak: PeakDominanceProof,
}

/// Why the conservative prover did not establish dominance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DominanceObstacle {
    /// Boundary counts, direct-feedback relation, or transfer behavior differ.
    BehaviorMismatch,
    /// Node-type inventories differ, so fixed-profile futures are not interchangeable.
    ProfileMismatch,
    /// Dominating cost is lexicographically worse.
    Cost,
    /// Domain containment was not proved.
    Domain,
    /// Pointwise internal-peak dominance was not proved.
    Peak,
}

/// One-sided exact component-dominance result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DominanceDecision {
    /// Every required dominance condition has an exact certificate.
    Proven(ComponentDominanceProof),
    /// No sound proof was found. The candidate must be retained.
    Unknown(DominanceObstacle),
}

/// Attempts to prove that `dominating` can replace `dominated` safely.
///
/// Besides the plan's domain, lexicographic-cost, and peak conditions, this
/// macro-safe relation requires identical node profiles. A different profile
/// changes the remaining fixed-profile search problem, so cross-profile
/// implementations stay in the frontier even when one has a lower total cost.
#[must_use]
pub fn try_prove_dominance(dominating: &Component, dominated: &Component) -> DominanceDecision {
    try_prove_dominance_inner(dominating, dominated, true)
}

/// Attempts the plan's global symbolic Pareto-dominance proof.
///
/// Unlike [`try_prove_dominance`], this relation intentionally permits a
/// different physical node-type profile. It is suitable for the persistent
/// symbolic frontier, where implementations compete by total lexicographic
/// component cost. It must not be used as a same-fixed-profile substitution
/// proof inside a parent DFS; that narrower use requires
/// [`try_prove_dominance`].
#[must_use]
pub fn try_prove_global_dominance(
    dominating: &Component,
    dominated: &Component,
) -> DominanceDecision {
    try_prove_dominance_inner(dominating, dominated, false)
}

fn try_prove_dominance_inner(
    dominating: &Component,
    dominated: &Component,
    require_same_profile: bool,
) -> DominanceDecision {
    if require_same_profile && dominating.profile != dominated.profile {
        return DominanceDecision::Unknown(DominanceObstacle::ProfileMismatch);
    }
    if dominating.cost() > dominated.cost() {
        return DominanceDecision::Unknown(DominanceObstacle::Cost);
    }
    let mut saw_behavior = false;
    let mut saw_domain = false;
    for input_order in permutations(dominating.boundary.input_count()) {
        for output_order in permutations(dominating.boundary.output_count()) {
            let Ok(boundary) = dominating.boundary.permute(&input_order, &output_order) else {
                continue;
            };
            let Ok(transfer) = dominating
                .transfer
                .permute_rows(&output_order)
                .and_then(|matrix| matrix.permute_columns(&input_order))
            else {
                continue;
            };
            if boundary != dominated.boundary || transfer != dominated.transfer {
                continue;
            }
            saw_behavior = true;
            let Ok(domain) = dominating.domain.permute_columns(&input_order) else {
                continue;
            };
            let Some(domain_proof) = prove_domain_containment(&domain, &dominated.domain) else {
                continue;
            };
            saw_domain = true;
            let Ok(internal) = dominating.internal_flow_map.permute_columns(&input_order) else {
                continue;
            };
            let Some(peak) =
                prove_peak_dominance(&internal, &dominated.internal_flow_map, &dominated.domain)
            else {
                continue;
            };
            return DominanceDecision::Proven(ComponentDominanceProof {
                dominating_input_order: input_order,
                dominating_output_order: output_order,
                domain: domain_proof,
                peak,
            });
        }
    }
    if !saw_behavior {
        DominanceDecision::Unknown(DominanceObstacle::BehaviorMismatch)
    } else if !saw_domain {
        DominanceDecision::Unknown(DominanceObstacle::Domain)
    } else {
        DominanceDecision::Unknown(DominanceObstacle::Peak)
    }
}

/// Verifies a stored component-dominance proof from scratch.
#[must_use]
pub fn verify_component_dominance(
    proof: &ComponentDominanceProof,
    dominating: &Component,
    dominated: &Component,
) -> bool {
    verify_component_dominance_inner(proof, dominating, dominated, true)
}

/// Verifies a stored global cross-profile symbolic dominance certificate.
#[must_use]
pub fn verify_global_component_dominance(
    proof: &ComponentDominanceProof,
    dominating: &Component,
    dominated: &Component,
) -> bool {
    verify_component_dominance_inner(proof, dominating, dominated, false)
}

fn verify_component_dominance_inner(
    proof: &ComponentDominanceProof,
    dominating: &Component,
    dominated: &Component,
    require_same_profile: bool,
) -> bool {
    if (require_same_profile && dominating.profile != dominated.profile)
        || dominating.cost() > dominated.cost()
    {
        return false;
    }
    let Ok(boundary) = dominating.boundary.permute(
        &proof.dominating_input_order,
        &proof.dominating_output_order,
    ) else {
        return false;
    };
    let Ok(transfer) = dominating
        .transfer
        .permute_rows(&proof.dominating_output_order)
        .and_then(|matrix| matrix.permute_columns(&proof.dominating_input_order))
    else {
        return false;
    };
    let Ok(domain) = dominating
        .domain
        .permute_columns(&proof.dominating_input_order)
    else {
        return false;
    };
    let Ok(internal) = dominating
        .internal_flow_map
        .permute_columns(&proof.dominating_input_order)
    else {
        return false;
    };
    boundary == dominated.boundary
        && transfer == dominated.transfer
        && verify_domain_containment(&proof.domain, &domain, &dominated.domain)
        && verify_peak_dominance(
            &proof.peak,
            &internal,
            &dominated.internal_flow_map,
            &dominated.domain,
        )
}

/// Result of inserting into a deterministic Pareto frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontierInsertion {
    /// The component was retained and these canonical keys were removed.
    Retained {
        /// Components proven dominated by the inserted component.
        removed: Vec<ComponentCanonicalKey>,
    },
    /// An existing component safely dominates the candidate.
    Dominated {
        /// Deterministic retained dominator.
        by: Box<ComponentCanonicalKey>,
        /// Exact verified proof.
        proof: Box<ComponentDominanceProof>,
    },
    /// The exact same canonical component was already present.
    Duplicate,
}

/// Deterministically ordered conservative component Pareto frontier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentParetoFrontier {
    components: Vec<Component>,
}

impl ComponentParetoFrontier {
    /// Creates an empty frontier.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Returns retained components in canonical-key order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Component> {
        self.components.iter()
    }

    /// Inserts one component, removing only alternatives with verified proofs.
    ///
    /// Mutual dominance is resolved by canonical key, so insertion order and
    /// worker scheduling cannot change the retained representative.
    pub fn insert(&mut self, component: Component) -> FrontierInsertion {
        if self
            .components
            .binary_search_by(|candidate| candidate.canonical_key.cmp(&component.canonical_key))
            .is_ok()
        {
            return FrontierInsertion::Duplicate;
        }

        let mut mutually_dominated = Vec::new();
        for existing in &self.components {
            let DominanceDecision::Proven(existing_proof) =
                try_prove_dominance(existing, &component)
            else {
                continue;
            };
            if let DominanceDecision::Proven(_) = try_prove_dominance(&component, existing) {
                if existing.canonical_key <= component.canonical_key {
                    return FrontierInsertion::Dominated {
                        by: Box::new(existing.canonical_key.clone()),
                        proof: Box::new(existing_proof),
                    };
                }
                mutually_dominated.push(existing.canonical_key.clone());
            } else {
                return FrontierInsertion::Dominated {
                    by: Box::new(existing.canonical_key.clone()),
                    proof: Box::new(existing_proof),
                };
            }
        }

        let mut removed = mutually_dominated;
        for existing in &self.components {
            if removed.contains(&existing.canonical_key) {
                continue;
            }
            if let DominanceDecision::Proven(_) = try_prove_dominance(&component, existing) {
                removed.push(existing.canonical_key.clone());
            }
        }
        removed.sort();
        removed.dedup();
        self.components
            .retain(|existing| !removed.contains(&existing.canonical_key));
        let insertion = self
            .components
            .binary_search_by(|existing| existing.canonical_key.cmp(&component.canonical_key))
            .unwrap_or_else(std::convert::identity);
        self.components.insert(insertion, component);
        FrontierInsertion::Retained { removed }
    }
}

/// Deterministic persistent Pareto frontier using the plan's cross-profile
/// component relation.
///
/// A candidate is removed only after exact domain, lexicographic-cost, and
/// universal internal-peak certificates verify. Failure to prove any condition
/// retains both alternatives. The narrower [`ComponentParetoFrontier`] remains
/// available for fixed-profile substitution catalogs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalComponentParetoFrontier {
    components: Vec<Component>,
}

impl GlobalComponentParetoFrontier {
    /// Creates an empty global symbolic frontier.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Returns retained records in canonical-key order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Component> {
        self.components.iter()
    }

    /// Inserts one component using only verified global dominance proofs.
    pub fn insert(&mut self, component: Component) -> FrontierInsertion {
        if self
            .components
            .binary_search_by(|candidate| candidate.canonical_key.cmp(&component.canonical_key))
            .is_ok()
        {
            return FrontierInsertion::Duplicate;
        }

        let mut mutually_dominated = Vec::new();
        for existing in &self.components {
            let DominanceDecision::Proven(existing_proof) =
                try_prove_global_dominance(existing, &component)
            else {
                continue;
            };
            if let DominanceDecision::Proven(_) = try_prove_global_dominance(&component, existing) {
                if existing.canonical_key <= component.canonical_key {
                    return FrontierInsertion::Dominated {
                        by: Box::new(existing.canonical_key.clone()),
                        proof: Box::new(existing_proof),
                    };
                }
                mutually_dominated.push(existing.canonical_key.clone());
            } else {
                return FrontierInsertion::Dominated {
                    by: Box::new(existing.canonical_key.clone()),
                    proof: Box::new(existing_proof),
                };
            }
        }

        let mut removed = mutually_dominated;
        for existing in &self.components {
            if removed.contains(&existing.canonical_key) {
                continue;
            }
            if let DominanceDecision::Proven(_) = try_prove_global_dominance(&component, existing) {
                removed.push(existing.canonical_key.clone());
            }
        }
        removed.sort();
        removed.dedup();
        self.components
            .retain(|existing| !removed.contains(&existing.canonical_key));
        let insertion = self
            .components
            .binary_search_by(|existing| existing.canonical_key.cmp(&component.canonical_key))
            .unwrap_or_else(std::convert::identity);
        self.components.insert(insertion, component);
        FrontierInsertion::Retained { removed }
    }
}

#[cfg(test)]
mod tests {
    use solver_api::{NodeId, NodeType, PhysicalNode};

    use super::*;
    use crate::{
        canonical::PartialTopology,
        scc::{
            DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
            FrozenSubsystemDeclaration, analyze_frozen_subsystem,
        },
        topology::TopologyState,
    };

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
        }
    }

    fn producer(node: u32, port: u8) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn consumer(node: u32, port: u8) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn topology(
        nodes: Vec<PhysicalNode>,
        links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    ) -> TopologyState {
        TopologyState::from_partial_topology(&PartialTopology {
            problem: Problem {
                inputs: vec![rational("2")],
                outputs: vec![rational("1"), rational("1")],
                max_link_rate: rational("10"),
            },
            nodes,
            links: links
                .into_iter()
                .map(|(producer, consumer)| PartialLink {
                    producer,
                    consumer,
                    flow: None,
                })
                .collect(),
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap()
    }

    fn symbolic(
        topology: &TopologyState,
        declaration: &FrozenSubsystemDeclaration,
    ) -> FrozenSubsystem {
        match analyze_frozen_subsystem(topology, declaration).unwrap() {
            FrozenSubsystemAnalysis::Symbolic(contract) => *contract,
            FrozenSubsystemAnalysis::Singular { .. } => panic!("expected symbolic component"),
        }
    }

    fn assert_internal_mapping(component: &Component) {
        assert_eq!(
            component.internal_flow_map().row_count(),
            component.witness_mapping().internal_link_order.len()
        );
        for (canonical_row, &frozen_link) in component
            .witness_mapping()
            .internal_link_order
            .iter()
            .enumerate()
        {
            let frozen = &component.frozen_witness().internal_links[frozen_link];
            let oriented = component
                .witness_mapping()
                .input_order
                .iter()
                .map(|&frozen_input| frozen.coefficients[frozen_input].clone())
                .collect::<Vec<_>>();
            assert_eq!(
                component.internal_flow_map().row(canonical_row),
                Some(oriented.as_slice())
            );
            assert_eq!(
                component.canonical_witness().internal_links[canonical_row].coefficients,
                oriented
            );
        }
    }

    fn splitter_component(reverse_outputs: bool) -> Component {
        let topology = topology(vec![node(0, NodeType::Splitter2)], Vec::new());
        let mut outputs = vec![
            DeclaredBoundaryOutput {
                port: producer(0, 0),
            },
            DeclaredBoundaryOutput {
                port: producer(0, 1),
            },
        ];
        if reverse_outputs {
            outputs.reverse();
        }
        Component::from_frozen(symbolic(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: consumer(0, 0),
                }],
                boundary_outputs: outputs,
            },
        ))
        .unwrap()
    }

    fn feedback_component_variant(swap_node_roles: bool, reverse_links: bool) -> Component {
        let (splitter, merger) = if swap_node_roles { (1, 0) } else { (0, 1) };
        let mut links = vec![
            (producer(merger, 0), consumer(splitter, 0)),
            (producer(splitter, 0), consumer(merger, 0)),
        ];
        if reverse_links {
            links.reverse();
        }
        let topology = topology(
            if swap_node_roles {
                vec![node(0, NodeType::Merger2), node(1, NodeType::Splitter2)]
            } else {
                vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)]
            },
            links,
        );
        Component::from_frozen(symbolic(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(splitter), NodeId(merger)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: consumer(merger, 1),
                }],
                boundary_outputs: vec![DeclaredBoundaryOutput {
                    port: producer(splitter, 1),
                }],
            },
        ))
        .unwrap()
    }

    fn feedback_component() -> Component {
        feedback_component_variant(false, false)
    }

    fn asymmetric_split_merge_component(
        swap_node_roles: bool,
        reverse_inputs: bool,
        reverse_outputs: bool,
    ) -> Component {
        let (splitter, merger) = if swap_node_roles { (1, 0) } else { (0, 1) };
        let nodes = if swap_node_roles {
            vec![node(0, NodeType::Merger2), node(1, NodeType::Splitter2)]
        } else {
            vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)]
        };
        let topology = topology(nodes, vec![(producer(splitter, 0), consumer(merger, 0))]);
        let mut inputs = vec![
            DeclaredBoundaryInput {
                port: consumer(splitter, 0),
            },
            DeclaredBoundaryInput {
                port: consumer(merger, 1),
            },
        ];
        let mut outputs = vec![
            DeclaredBoundaryOutput {
                port: producer(splitter, 1),
            },
            DeclaredBoundaryOutput {
                port: producer(merger, 0),
            },
        ];
        if reverse_inputs {
            inputs.reverse();
        }
        if reverse_outputs {
            outputs.reverse();
        }
        Component::from_frozen(symbolic(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(splitter), NodeId(merger)],
                boundary_inputs: inputs,
                boundary_outputs: outputs,
            },
        ))
        .unwrap()
    }

    #[test]
    fn splitter_component_is_canonical_under_symmetric_output_order() {
        let left = splitter_component(false);
        let right = splitter_component(true);
        assert!(left.is_projection_certified());
        assert!(right.is_projection_certified());
        assert_eq!(left.behavior_key(), right.behavior_key());
        assert_eq!(left.canonical_key(), right.canonical_key());
        assert_eq!(left.transfer().row(0).unwrap(), &[rational("1/2")]);
        assert_eq!(left.transfer().row(1).unwrap(), &[rational("1/2")]);
        assert_eq!(
            (0..2)
                .map(|output| left.boundary().forbids_direct_feedback(0, output))
                .collect::<Vec<_>>(),
            vec![Some(true), Some(true)]
        );
        assert!(matches!(
            left.capacity_behavior().transparency(),
            CapacityTransparency::Proven(_)
        ));
    }

    #[test]
    fn projection_certification_rejects_a_tampered_transfer_row() {
        let component = splitter_component(false);
        let tampered_transfer =
            ExactMatrix::from_rows(1, vec![vec![rational("2/3")], vec![rational("1/2")]]).unwrap();

        assert_eq!(
            certify_projected_equivalence(
                component.canonical_witness(),
                component.domain(),
                &tampered_transfer,
                component.internal_flow_map(),
            ),
            Err(ComponentError::ProjectionEquivalenceMismatch)
        );
        assert!(component.is_projection_certified());
    }

    #[test]
    fn projection_certification_rejects_a_tampered_strict_domain() {
        let component = splitter_component(false);
        let tampered_domain =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![rational("-1")]]).unwrap();

        assert_eq!(
            certify_projected_equivalence(
                component.canonical_witness(),
                &tampered_domain,
                component.transfer(),
                component.internal_flow_map(),
            ),
            Err(ComponentError::ProjectionEquivalenceMismatch)
        );
    }

    #[test]
    fn persistence_record_round_trips_only_through_physical_reconstruction() {
        for component in [splitter_component(false), feedback_component()] {
            let record = component.export_record();
            let witness_only =
                Component::import_canonical_witness(&record.canonical_witness).unwrap();
            assert_eq!(witness_only.export_record(), record);
            let imported = Component::import_verified(&record).unwrap();
            assert!(imported.is_projection_certified());
            assert_eq!(imported.export_record(), record);
            assert_eq!(imported.canonical_key(), component.canonical_key());
        }
    }

    #[test]
    fn persistence_import_rejects_algebra_capacity_and_witness_tampering() {
        let component = splitter_component(false);

        let mut transfer = component.export_record();
        transfer.transfer =
            ExactMatrix::from_rows(1, vec![vec![rational("2/3")], vec![rational("1/2")]]).unwrap();
        assert!(matches!(
            Component::import_verified(&transfer),
            Err(ComponentError::RecordMismatch("transfer"))
        ));

        let mut domain = component.export_record();
        domain.domain =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![rational("-1")]]).unwrap();
        assert!(matches!(
            Component::import_verified(&domain),
            Err(ComponentError::RecordMismatch("domain"))
        ));

        let mut capacity = component.export_record();
        capacity.capacity.transparency = CapacityTransparency::Unknown;
        assert!(matches!(
            Component::import_verified(&capacity),
            Err(ComponentError::RecordMismatch("capacity"))
        ));

        let mut coefficients = feedback_component().export_record();
        coefficients.canonical_witness.internal_links[0].coefficients[0] = rational("99");
        assert!(Component::import_verified(&coefficients).is_err());

        let mut endpoint = component.export_record();
        endpoint.canonical_witness.boundary_inputs[0] =
            ConsumerPortRef::Discard(solver_api::DiscardTerminalIndex(0));
        assert!(matches!(
            Component::import_verified(&endpoint),
            Err(ComponentError::InvalidRecordWitness(_))
        ));
    }

    #[test]
    fn exact_capacity_metrics_and_equality_boundary_are_enforced() {
        let component = feedback_component();
        let application = component
            .evaluate(&[rational("1")], &rational("2"))
            .unwrap();
        assert_eq!(application.outputs, vec![rational("1")]);
        assert_eq!(application.capacity.internal_peak, rational("2"));
        assert_eq!(application.capacity.boundary_peak, rational("1"));
        assert_eq!(application.capacity.alpha, rational("2"));
        assert_eq!(
            component.internal_flow_map().row_count(),
            component.canonical_witness().internal_links.len()
        );
        for (row, link) in component
            .internal_flow_map()
            .rows()
            .zip(&component.canonical_witness().internal_links)
        {
            assert_eq!(row, link.coefficients);
        }
        assert!(matches!(
            component.evaluate(&[rational("1")], &rational("3/2")),
            Err(ComponentApplicationError::InternalCapacityExceeded { .. })
        ));
        assert!(matches!(
            component.capacity_behavior().transparency(),
            CapacityTransparency::Unknown
        ));
        let mut application_classifier = component.capacity_behavior().clone();
        assert!(
            application_classifier
                .record_counterexample(component.domain(), &[rational("1")])
                .unwrap()
        );
        assert!(matches!(
            application_classifier.transparency(),
            CapacityTransparency::Disproven { .. }
        ));
        assert!(matches!(
            component.capacity_behavior().transparency(),
            CapacityTransparency::Unknown
        ));
    }

    #[test]
    fn component_key_and_canonical_witness_ignore_node_roles_and_link_storage_order() {
        let baseline = feedback_component_variant(false, false);
        let relabelled = feedback_component_variant(true, true);

        assert_eq!(baseline.behavior_key(), relabelled.behavior_key());
        assert_eq!(baseline.canonical_key(), relabelled.canonical_key());
        assert_eq!(baseline.canonical_witness(), relabelled.canonical_witness());
        assert_internal_mapping(&baseline);
        assert_internal_mapping(&relabelled);
        assert_eq!(
            baseline.canonical_witness().nodes,
            vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)]
        );

        for link in &baseline.canonical_witness().internal_links {
            let producer_node = match link.producer {
                ProducerPortRef::Node { node, .. } => node,
                ProducerPortRef::Input(_) => panic!("component link producer must be internal"),
            };
            let producer_type =
                baseline.canonical_witness().nodes[producer_node.0 as usize].node_type;
            let expected = match producer_type {
                NodeType::Splitter2 => rational("1"),
                NodeType::Merger2 => rational("2"),
                NodeType::Splitter3 | NodeType::Merger3 => {
                    panic!("fixture contains only two-port nodes")
                }
            };
            assert_eq!(link.coefficients, vec![expected]);
        }
    }

    #[test]
    fn asymmetric_feedback_signature_and_witness_are_canonical_under_boundary_permutations() {
        let baseline = asymmetric_split_merge_component(false, false, false);
        for variant in [
            asymmetric_split_merge_component(false, true, false),
            asymmetric_split_merge_component(false, false, true),
            asymmetric_split_merge_component(true, true, true),
        ] {
            assert_eq!(baseline.behavior_key(), variant.behavior_key());
            assert_eq!(baseline.canonical_key(), variant.canonical_key());
            assert_eq!(baseline.canonical_witness(), variant.canonical_witness());
        }

        let witness = baseline.canonical_witness();
        assert_internal_mapping(&baseline);
        assert_eq!(witness.internal_links.len(), 1);
        assert_eq!(
            witness.internal_links[0].coefficients,
            baseline.internal_flow_map().row(0).unwrap()
        );
        for (input, &consumer) in witness.boundary_inputs.iter().enumerate() {
            let forbidden = witness
                .boundary_outputs
                .iter()
                .enumerate()
                .map(|(output, &producer)| {
                    let expected = consumer_owner(consumer) == producer_owner(producer);
                    assert_eq!(
                        baseline.boundary().forbids_direct_feedback(input, output),
                        Some(expected)
                    );
                    expected
                })
                .collect::<Vec<_>>();
            assert_eq!(forbidden.iter().filter(|&&value| value).count(), 1);
        }
        for output in 0..witness.boundary_outputs.len() {
            assert_eq!(
                (0..witness.boundary_inputs.len())
                    .filter(
                        |&input| baseline.boundary().forbids_direct_feedback(input, output)
                            == Some(true)
                    )
                    .count(),
                1
            );
        }

        let internal_producer = producer_owner(witness.internal_links[0].producer).unwrap();
        for (input, boundary) in witness.boundary_inputs.iter().copied().enumerate() {
            let coefficient = &witness.internal_links[0].coefficients[input];
            if consumer_owner(boundary) == Some(internal_producer) {
                assert_eq!(coefficient, &rational("1/2"));
            } else {
                assert!(coefficient.is_zero());
            }
        }
    }

    #[test]
    fn transparency_certificate_and_counterexample_are_both_exact() {
        let domain =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![Rational::one()]]).unwrap();
        let transfer = ExactMatrix::from_rows(1, vec![vec![Rational::one()]]).unwrap();
        let transparent_internal = ExactMatrix::from_rows(1, vec![vec![rational("1/2")]]).unwrap();
        let transparent = CapacityBehavior::new(&domain, transparent_internal, &transfer);
        let CapacityTransparency::Proven(proof) = transparent.transparency() else {
            panic!("expected exact transparency proof");
        };
        assert!(verify_transparency_proof(
            proof,
            &domain,
            transparent.internal_rows(),
            transparent.boundary_rows()
        ));

        let mut nontransparent = CapacityBehavior::new(
            &domain,
            ExactMatrix::from_rows(1, vec![vec![rational("2")]]).unwrap(),
            &transfer,
        );
        assert!(matches!(
            nontransparent.transparency(),
            CapacityTransparency::Unknown
        ));
        assert!(
            nontransparent
                .record_counterexample(&domain, &[rational("7/3")])
                .unwrap()
        );
        let CapacityTransparency::Disproven {
            internal_peak,
            boundary_peak,
            ..
        } = nontransparent.transparency()
        else {
            panic!("expected exact counterexample");
        };
        assert_eq!(internal_peak, &rational("14/3"));
        assert_eq!(boundary_peak, &rational("7/3"));
    }

    #[test]
    fn domain_containment_uses_only_verified_row_consequences() {
        let broad = FeasibilityDomain::new(
            ExactMatrix::empty(2),
            vec![vec![Rational::one(), Rational::zero()]],
        )
        .unwrap();
        let narrow = FeasibilityDomain::new(
            ExactMatrix::empty(2),
            vec![
                vec![Rational::one(), Rational::zero()],
                vec![Rational::zero(), Rational::one()],
            ],
        )
        .unwrap();
        let proof = prove_domain_containment(&broad, &narrow).unwrap();
        assert!(verify_domain_containment(&proof, &broad, &narrow));
        assert!(prove_domain_containment(&narrow, &broad).is_none());

        let equality = FeasibilityDomain::new(
            ExactMatrix::from_rows(2, vec![vec![rational("2"), rational("-2")]]).unwrap(),
            vec![vec![Rational::one(), Rational::one()]],
        )
        .unwrap();
        assert!(prove_domain_containment(&equality, &equality).is_some());
    }

    #[test]
    fn empty_domain_never_dominates_a_nonempty_domain() {
        let empty =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![Rational::zero()]]).unwrap();
        let nonempty =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![Rational::one()]]).unwrap();
        assert!(empty.is_proven_empty());
        assert!(prove_domain_containment(&empty, &nonempty).is_none());
        assert!(!verify_domain_containment(
            &DomainContainmentProof {
                equality_proofs: Vec::new(),
                strict_proofs: Vec::new(),
            },
            &empty,
            &nonempty,
        ));

        let proof = prove_domain_containment(&nonempty, &empty)
            .expect("the empty domain is a subset of every compatible domain");
        assert!(verify_domain_containment(&proof, &nonempty, &empty));
    }

    #[test]
    fn empty_domain_component_cannot_eliminate_an_applicable_frontier_alternative() {
        let applicable = feedback_component();
        let mut empty = applicable.clone();
        empty.domain =
            FeasibilityDomain::new(ExactMatrix::empty(1), vec![vec![Rational::zero()]]).unwrap();
        empty.capacity = CapacityBehavior::new(
            &empty.domain,
            empty.internal_flow_map.clone(),
            &empty.transfer,
        );
        empty.canonical_key.domain = empty.domain.clone();
        empty.canonical_key.projection.push(17);

        assert!(empty.domain.is_proven_empty());
        assert_eq!(
            try_prove_dominance(&empty, &applicable),
            DominanceDecision::Unknown(DominanceObstacle::Domain)
        );
        assert_eq!(
            try_prove_global_dominance(&empty, &applicable),
            DominanceDecision::Unknown(DominanceObstacle::Domain)
        );
        let DominanceDecision::Proven(local_proof) = try_prove_dominance(&applicable, &empty)
        else {
            panic!("an applicable equal-cost implementation must dominate an empty alternative")
        };
        assert!(verify_component_dominance(
            &local_proof,
            &applicable,
            &empty
        ));
        let DominanceDecision::Proven(global_proof) =
            try_prove_global_dominance(&applicable, &empty)
        else {
            panic!("global empty-domain elimination must have an exact proof")
        };
        assert!(verify_global_component_dominance(
            &global_proof,
            &applicable,
            &empty
        ));

        let applicable_key = applicable.canonical_key().clone();
        for reverse in [false, true] {
            let mut local = ComponentParetoFrontier::new();
            let mut global = GlobalComponentParetoFrontier::new();
            let order = if reverse {
                [applicable.clone(), empty.clone()]
            } else {
                [empty.clone(), applicable.clone()]
            };
            for component in order {
                local.insert(component.clone());
                global.insert(component);
            }
            assert_eq!(
                local
                    .iter()
                    .map(Component::canonical_key)
                    .collect::<Vec<_>>(),
                vec![&applicable_key]
            );
            assert_eq!(
                global
                    .iter()
                    .map(Component::canonical_key)
                    .collect::<Vec<_>>(),
                vec![&applicable_key]
            );
        }
    }

    fn with_uniform_internal(mut component: Component, value: &str, key_byte: u8) -> Component {
        let rows = vec![vec![rational(value)]; component.internal_links as usize];
        component.internal_flow_map =
            ExactMatrix::from_rows(component.boundary.input_count(), rows).unwrap();
        component.capacity = CapacityBehavior::new(
            &component.domain,
            component.internal_flow_map.clone(),
            &component.transfer,
        );
        component.canonical_key.internal_flow_map = component.internal_flow_map.clone();
        component.canonical_key.projection.push(key_byte);
        component
    }

    #[test]
    fn dominance_proves_domain_cost_and_peak_in_the_safe_direction_only() {
        let lower_peak = with_uniform_internal(feedback_component(), "1", 1);
        let higher_peak = with_uniform_internal(feedback_component(), "2", 2);
        let DominanceDecision::Proven(proof) = try_prove_dominance(&lower_peak, &higher_peak)
        else {
            panic!("lower exact peak should dominate");
        };
        assert!(verify_component_dominance(
            &proof,
            &lower_peak,
            &higher_peak
        ));
        assert_eq!(
            try_prove_dominance(&higher_peak, &lower_peak),
            DominanceDecision::Unknown(DominanceObstacle::Peak)
        );
    }

    #[test]
    fn pareto_frontier_is_insertion_order_independent_and_keeps_profiles() {
        let lower_peak = with_uniform_internal(feedback_component(), "1", 1);
        let higher_peak = with_uniform_internal(feedback_component(), "2", 2);
        let expected = lower_peak.canonical_key().clone();

        let mut left = ComponentParetoFrontier::new();
        assert!(matches!(
            left.insert(higher_peak.clone()),
            FrontierInsertion::Retained { .. }
        ));
        assert!(matches!(
            left.insert(lower_peak.clone()),
            FrontierInsertion::Retained { .. }
        ));

        let mut right = ComponentParetoFrontier::new();
        assert!(matches!(
            right.insert(lower_peak.clone()),
            FrontierInsertion::Retained { .. }
        ));
        assert!(matches!(
            right.insert(higher_peak),
            FrontierInsertion::Dominated { .. }
        ));
        assert_eq!(
            left.iter()
                .map(Component::canonical_key)
                .collect::<Vec<_>>(),
            vec![&expected]
        );
        assert_eq!(
            right
                .iter()
                .map(Component::canonical_key)
                .collect::<Vec<_>>(),
            vec![&expected]
        );

        let mut different_profile = lower_peak.clone();
        different_profile.profile.splitter2 -= 1;
        different_profile.profile.splitter3 += 1;
        different_profile.canonical_key.profile = different_profile.profile;
        different_profile.canonical_key.projection.push(9);
        assert_eq!(
            try_prove_dominance(&lower_peak, &different_profile),
            DominanceDecision::Unknown(DominanceObstacle::ProfileMismatch)
        );
        let mut frontier = ComponentParetoFrontier::new();
        frontier.insert(lower_peak);
        frontier.insert(different_profile);
        assert_eq!(frontier.iter().len(), 2);
    }

    #[test]
    fn global_pareto_is_cross_profile_while_fixed_profile_substitution_is_not() {
        let lower_peak = with_uniform_internal(feedback_component(), "1", 1);
        let mut other_profile = lower_peak.clone();
        other_profile.profile.splitter2 -= 1;
        other_profile.profile.splitter3 += 1;
        other_profile.canonical_key.profile = other_profile.profile;
        other_profile.canonical_key.projection.push(9);

        assert_eq!(
            try_prove_dominance(&lower_peak, &other_profile),
            DominanceDecision::Unknown(DominanceObstacle::ProfileMismatch)
        );
        let DominanceDecision::Proven(proof) =
            try_prove_global_dominance(&lower_peak, &other_profile)
        else {
            panic!("global component costs may compare across node-type profiles");
        };
        assert!(verify_global_component_dominance(
            &proof,
            &lower_peak,
            &other_profile
        ));

        let expected =
            std::cmp::min(lower_peak.canonical_key(), other_profile.canonical_key()).clone();
        let mut left = GlobalComponentParetoFrontier::new();
        left.insert(other_profile.clone());
        left.insert(lower_peak.clone());
        let mut right = GlobalComponentParetoFrontier::new();
        right.insert(lower_peak);
        right.insert(other_profile);
        assert_eq!(
            left.iter()
                .map(Component::canonical_key)
                .collect::<Vec<_>>(),
            vec![&expected]
        );
        assert_eq!(
            right
                .iter()
                .map(Component::canonical_key)
                .collect::<Vec<_>>(),
            vec![&expected]
        );
    }
}
