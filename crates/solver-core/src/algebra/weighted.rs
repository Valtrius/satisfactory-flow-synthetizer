//! Exact rollback weighted union-find for multiplicative flow relations.
//!
//! Every non-root entry stores an invariant of the form
//! `value(entry) = weight_to_parent * value(parent)`. Multiplying those exact
//! weights along a parent chain therefore proves every ratio returned by this
//! module. Roots may additionally carry an exact known constant. Union by size
//! bounds tree depth; equal-size roots are selected by stable [`FlowVarId`], so
//! argument order cannot affect the representative. Path compression is
//! deliberately absent: every mutation is small, explicit, and reversible.
//!
//! A mismatched ratio cycle is not automatically inconsistent. It proves that
//! the component's root is zero. Only a conflicting known nonzero value makes
//! that cycle a contradiction. Positivity and capacity policy belongs to the
//! inequality layer, not this purely algebraic data structure.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use solver_api::Rational;
use thiserror::Error;

use crate::topology::FlowVarId;

/// Whether an exact constraint changed the represented solution set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintOutcome {
    /// The constraint merged components or established a new exact constant.
    Changed,
    /// The constraint was already implied by the stored facts.
    AlreadySatisfied,
    /// The constraint conflicts with a known exact fact.
    ///
    /// This outcome is transactional: the union-find is byte-for-byte
    /// unchanged relative to its public semantic state and rollback trail.
    Contradiction,
}

impl ConstraintOutcome {
    /// Returns whether the constraint leaves at least one algebraic solution.
    #[must_use]
    pub const fn is_consistent(self) -> bool {
        !matches!(self, Self::Contradiction)
    }
}

/// A malformed weighted-union-find operation.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WeightedError {
    /// An operation referred to a variable that has not been added.
    #[error("unknown flow variable {0:?}")]
    UnknownVariable(FlowVarId),
    /// A zero multiplier cannot form an invertible union-find relation.
    ///
    /// Call [`WeightedUnionFind::assign`] with zero instead: `x = 0 * y`
    /// constrains only `x` and does not place `x` and `y` in one ratio class.
    #[error("a weighted relation requires a nonzero multiplier")]
    ZeroRatio,
}

/// Opaque position in a [`WeightedUnionFind`] rollback trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedCheckpoint {
    trail_len: usize,
    dirty_roots: BTreeSet<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    variable: FlowVarId,
    parent: usize,
    weight_to_parent: Rational,
    component_size: usize,
    known_root_value: Option<Rational>,
    next: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Undo {
    Added {
        variable: FlowVarId,
        index: usize,
    },
    RootValue {
        root: usize,
        previous: Option<Rational>,
    },
    Merged {
        child: usize,
        parent: usize,
        child_parent: usize,
        child_weight: Rational,
        child_known: Option<Rational>,
        parent_size: usize,
        parent_known: Option<Rational>,
        parent_next: usize,
        child_next: usize,
    },
}

/// Rollback-capable exact ratio classes over physical flow variables.
///
/// Constraint methods never apply positivity or capacity policy. In
/// particular, known zero and negative values are valid algebraic facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeightedUnionFind {
    indices: BTreeMap<FlowVarId, usize>,
    nodes: Vec<Node>,
    trail: Vec<Undo>,
    dirty_roots: BTreeSet<usize>,
}

impl WeightedUnionFind {
    /// Creates an empty exact ratio structure.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            indices: BTreeMap::new(),
            nodes: Vec::new(),
            trail: Vec::new(),
            dirty_roots: BTreeSet::new(),
        }
    }

    /// Returns the number of currently registered flow variables.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether no flow variables are currently registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns whether `variable` is currently registered.
    #[must_use]
    pub fn contains(&self, variable: FlowVarId) -> bool {
        self.indices.contains_key(&variable)
    }

    /// Adds an unconstrained variable.
    ///
    /// Returns `false` without changing state if the variable already exists.
    #[must_use]
    pub fn add_variable(&mut self, variable: FlowVarId) -> bool {
        if self.contains(variable) {
            return false;
        }

        let index = self.nodes.len();
        self.nodes.push(Node {
            variable,
            parent: index,
            weight_to_parent: Rational::one(),
            component_size: 1,
            known_root_value: None,
            next: index,
        });
        let previous = self.indices.insert(variable, index);
        debug_assert!(previous.is_none());
        self.trail.push(Undo::Added { variable, index });
        self.dirty_roots.insert(index);
        true
    }

    /// Captures the current rollback position in constant time.
    #[must_use]
    pub fn checkpoint(&self) -> WeightedCheckpoint {
        WeightedCheckpoint {
            trail_len: self.trail.len(),
            dirty_roots: self.dirty_roots.clone(),
        }
    }

    /// Restores every mutation made after `checkpoint`, including variable
    /// additions.
    ///
    /// # Panics
    ///
    /// Panics if the checkpoint lies beyond the current trail, which indicates
    /// a stale checkpoint reused after rolling back past it or a checkpoint
    /// taken from another instance.
    pub fn rollback(&mut self, checkpoint: WeightedCheckpoint) {
        assert!(
            checkpoint.trail_len <= self.trail.len(),
            "weighted-union-find checkpoint lies beyond the current trail"
        );

        while self.trail.len() > checkpoint.trail_len {
            let undo = self
                .trail
                .pop()
                .expect("trail length was checked before rollback");
            match undo {
                Undo::Added { variable, index } => {
                    assert_eq!(index + 1, self.nodes.len());
                    let removed_node = self.nodes.pop().expect("added node must still exist");
                    assert_eq!(removed_node.variable, variable);
                    assert_eq!(self.indices.remove(&variable), Some(index));
                }
                Undo::RootValue { root, previous } => {
                    self.nodes[root].known_root_value = previous;
                }
                Undo::Merged {
                    child,
                    parent,
                    child_parent,
                    child_weight,
                    child_known,
                    parent_size,
                    parent_known,
                    parent_next,
                    child_next,
                } => {
                    self.nodes[parent].component_size = parent_size;
                    self.nodes[parent].known_root_value = parent_known;
                    self.nodes[child].parent = child_parent;
                    self.nodes[child].weight_to_parent = child_weight;
                    self.nodes[child].known_root_value = child_known;
                    self.nodes[parent].next = parent_next;
                    self.nodes[child].next = child_next;
                }
            }
        }
        self.dirty_roots = checkpoint.dirty_roots;
    }

    /// Adds the exact relation `value(left) = factor * value(right)`.
    ///
    /// A contradictory result performs no mutation. If a new cycle has a
    /// different coefficient product, it establishes exact zero for the whole
    /// component instead of assuming positivity and rejecting the cycle.
    ///
    /// # Errors
    ///
    /// Returns [`WeightedError::UnknownVariable`] for an unregistered endpoint
    /// and [`WeightedError::ZeroRatio`] for a zero factor.
    pub fn relate(
        &mut self,
        left: FlowVarId,
        factor: &Rational,
        right: FlowVarId,
    ) -> Result<ConstraintOutcome, WeightedError> {
        let left_index = self.index_of(left)?;
        let right_index = self.index_of(right)?;
        if factor.is_zero() {
            return Err(WeightedError::ZeroRatio);
        }

        let (left_root, left_weight) = self.root_and_weight(left_index);
        let (right_root, right_weight) = self.root_and_weight(right_index);
        if left_root == right_root {
            let required_left_weight = factor * &right_weight;
            if left_weight == required_left_weight {
                return Ok(ConstraintOutcome::AlreadySatisfied);
            }
            return Ok(self.force_root_zero(left_root));
        }

        let left_known = self.nodes[left_root]
            .known_root_value
            .as_ref()
            .map(|known| &left_weight * known);
        let right_known = self.nodes[right_root]
            .known_root_value
            .as_ref()
            .map(|known| &right_weight * known);
        if let (Some(left_value), Some(right_value)) = (&left_known, &right_known)
            && left_value != &(factor * right_value)
        {
            return Ok(ConstraintOutcome::Contradiction);
        }

        let left_is_parent = match self.nodes[left_root]
            .component_size
            .cmp(&self.nodes[right_root].component_size)
        {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => self.nodes[left_root].variable < self.nodes[right_root].variable,
        };

        if left_is_parent {
            let denominator = factor * &right_weight;
            let right_to_left = &left_weight / &denominator;
            self.merge_roots(right_root, left_root, right_to_left);
        } else {
            let numerator = factor * &right_weight;
            let left_to_right = &numerator / &left_weight;
            self.merge_roots(left_root, right_root, left_to_right);
        }
        Ok(ConstraintOutcome::Changed)
    }

    /// Assigns one variable to an exact constant.
    ///
    /// Zero and negative constants are retained without policy pruning. A
    /// contradictory result performs no mutation.
    ///
    /// # Errors
    ///
    /// Returns [`WeightedError::UnknownVariable`] if `variable` has not been
    /// added.
    pub fn assign(
        &mut self,
        variable: FlowVarId,
        value: &Rational,
    ) -> Result<ConstraintOutcome, WeightedError> {
        let index = self.index_of(variable)?;
        let (root, weight) = self.root_and_weight(index);
        let root_value = value / &weight;
        match &self.nodes[root].known_root_value {
            Some(known) if known == &root_value => Ok(ConstraintOutcome::AlreadySatisfied),
            Some(_) => Ok(ConstraintOutcome::Contradiction),
            None => {
                self.trail.push(Undo::RootValue {
                    root,
                    previous: None,
                });
                self.nodes[root].known_root_value = Some(root_value);
                self.dirty_roots.insert(root);
                Ok(ConstraintOutcome::Changed)
            }
        }
    }

    /// Returns the exact stored relation `left = factor * right` when both
    /// variables belong to one ratio component.
    ///
    /// A component known to be zero can satisfy more than one ratio; in that
    /// case this returns the deterministic spanning-tree factor, which remains
    /// a valid relation but is not claimed to be unique.
    ///
    /// # Errors
    ///
    /// Returns [`WeightedError::UnknownVariable`] for an unregistered endpoint.
    pub fn ratio(
        &self,
        left: FlowVarId,
        right: FlowVarId,
    ) -> Result<Option<Rational>, WeightedError> {
        let left_index = self.index_of(left)?;
        let right_index = self.index_of(right)?;
        let (left_root, left_weight) = self.root_and_weight(left_index);
        let (right_root, right_weight) = self.root_and_weight(right_index);
        Ok((left_root == right_root).then(|| &left_weight / &right_weight))
    }

    /// Returns `(root, factor)` proving `variable = factor * root`.
    ///
    /// The root is the current deterministic representative selected by union
    /// by size with a stable [`FlowVarId`] tie-break.
    ///
    /// # Errors
    ///
    /// Returns [`WeightedError::UnknownVariable`] if `variable` has not been
    /// added.
    pub fn representative(
        &self,
        variable: FlowVarId,
    ) -> Result<(FlowVarId, Rational), WeightedError> {
        let index = self.index_of(variable)?;
        let (root, factor) = self.root_and_weight(index);
        Ok((self.nodes[root].variable, factor))
    }

    /// Returns a variable's exact value when its component has a known root.
    ///
    /// # Errors
    ///
    /// Returns [`WeightedError::UnknownVariable`] if `variable` has not been
    /// added.
    pub fn known_value(&self, variable: FlowVarId) -> Result<Option<Rational>, WeightedError> {
        let index = self.index_of(variable)?;
        let (root, factor) = self.root_and_weight(index);
        Ok(self.nodes[root]
            .known_root_value
            .as_ref()
            .map(|known| &factor * known))
    }

    fn index_of(&self, variable: FlowVarId) -> Result<usize, WeightedError> {
        self.indices
            .get(&variable)
            .copied()
            .ok_or(WeightedError::UnknownVariable(variable))
    }

    fn root_and_weight(&self, start: usize) -> (usize, Rational) {
        let mut cursor = start;
        let mut factor = Rational::one();
        while self.nodes[cursor].parent != cursor {
            factor = factor * &self.nodes[cursor].weight_to_parent;
            cursor = self.nodes[cursor].parent;
        }
        (cursor, factor)
    }

    fn force_root_zero(&mut self, root: usize) -> ConstraintOutcome {
        match &self.nodes[root].known_root_value {
            Some(known) if known.is_zero() => ConstraintOutcome::AlreadySatisfied,
            Some(_) => ConstraintOutcome::Contradiction,
            None => {
                self.trail.push(Undo::RootValue {
                    root,
                    previous: None,
                });
                self.nodes[root].known_root_value = Some(Rational::zero());
                self.dirty_roots.insert(root);
                ConstraintOutcome::Changed
            }
        }
    }

    fn merge_roots(&mut self, child: usize, parent: usize, child_to_parent: Rational) {
        debug_assert_eq!(self.nodes[child].parent, child);
        debug_assert_eq!(self.nodes[parent].parent, parent);
        debug_assert!(!child_to_parent.is_zero());

        let child_parent = self.nodes[child].parent;
        let child_weight = self.nodes[child].weight_to_parent.clone();
        let child_known = self.nodes[child].known_root_value.clone();
        let parent_size = self.nodes[parent].component_size;
        let parent_known = self.nodes[parent].known_root_value.clone();
        let merged_parent_known = parent_known
            .clone()
            .or_else(|| child_known.as_ref().map(|known| known / &child_to_parent));

        let parent_next = self.nodes[parent].next;
        let child_next = self.nodes[child].next;
        self.trail.push(Undo::Merged {
            child,
            parent,
            child_parent,
            child_weight,
            child_known,
            parent_size,
            parent_known,
            parent_next,
            child_next,
        });
        self.nodes[child].parent = parent;
        self.nodes[child].weight_to_parent = child_to_parent;
        self.nodes[child].known_root_value = None;
        self.nodes[parent].component_size += self.nodes[child].component_size;
        self.nodes[parent].known_root_value = merged_parent_known;
        self.nodes[parent].next = child_next;
        self.nodes[child].next = parent_next;
        self.dirty_roots.remove(&child);
        self.dirty_roots.insert(parent);
    }

    pub(crate) fn is_bounds_dirty(&self) -> bool {
        !self.dirty_roots.is_empty()
    }

    pub(crate) fn clear_bounds_dirty(&mut self) {
        self.dirty_roots.clear();
    }

    pub(crate) fn dirty_variables(&self) -> Vec<FlowVarId> {
        let mut seen_roots = BTreeSet::new();
        let mut variables = Vec::new();
        for &index in &self.dirty_roots {
            let (root, _) = self.root_and_weight(index);
            if !seen_roots.insert(root) {
                continue;
            }
            let mut cursor = root;
            loop {
                variables.push(self.nodes[cursor].variable);
                cursor = self.nodes[cursor].next;
                if cursor == root {
                    break;
                }
            }
        }
        variables
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    use super::*;

    fn variable(id: u32) -> FlowVarId {
        FlowVarId(id)
    }

    fn rational(numerator: i64, denominator: i64) -> Rational {
        Rational::new(numerator, denominator).unwrap()
    }

    fn populated(count: u32) -> WeightedUnionFind {
        let mut union_find = WeightedUnionFind::new();
        for id in 0..count {
            assert!(union_find.add_variable(variable(id)));
        }
        union_find
    }

    #[test]
    fn exact_ratios_and_negative_known_values_propagate() {
        let mut union_find = populated(3);
        assert_eq!(
            union_find.relate(variable(0), &Rational::from(2), variable(1)),
            Ok(ConstraintOutcome::Changed)
        );
        assert_eq!(
            union_find.relate(variable(2), &Rational::from(3), variable(1)),
            Ok(ConstraintOutcome::Changed)
        );
        assert_eq!(
            union_find.ratio(variable(0), variable(2)).unwrap(),
            Some(rational(2, 3))
        );
        assert_eq!(
            union_find.assign(variable(1), &Rational::from(-5)),
            Ok(ConstraintOutcome::Changed)
        );
        assert_eq!(
            union_find.known_value(variable(0)).unwrap(),
            Some(Rational::from(-10))
        );
        assert_eq!(
            union_find.known_value(variable(2)).unwrap(),
            Some(Rational::from(-15))
        );
    }

    #[test]
    fn mismatched_cycle_forces_zero_before_it_can_contradict() {
        let mut union_find = populated(2);
        union_find
            .relate(variable(0), &Rational::from(2), variable(1))
            .unwrap();
        assert_eq!(
            union_find.relate(variable(0), &Rational::from(3), variable(1)),
            Ok(ConstraintOutcome::Changed)
        );
        assert_eq!(
            union_find.known_value(variable(0)).unwrap(),
            Some(Rational::zero())
        );
        assert_eq!(
            union_find.known_value(variable(1)).unwrap(),
            Some(Rational::zero())
        );

        let before = union_find.clone();
        assert_eq!(
            union_find.assign(variable(0), &Rational::one()),
            Ok(ConstraintOutcome::Contradiction)
        );
        assert_eq!(union_find, before);
    }

    #[test]
    fn known_nonzero_mismatched_cycle_is_transactional() {
        let mut union_find = populated(2);
        union_find
            .relate(variable(0), &Rational::from(2), variable(1))
            .unwrap();
        union_find.assign(variable(1), &Rational::from(-7)).unwrap();
        let before = union_find.clone();
        assert_eq!(
            union_find.relate(variable(0), &Rational::from(3), variable(1)),
            Ok(ConstraintOutcome::Contradiction)
        );
        assert_eq!(union_find, before);

        assert_eq!(
            union_find.relate(variable(0), &Rational::zero(), variable(1)),
            Err(WeightedError::ZeroRatio)
        );
        assert_eq!(union_find, before);
    }

    #[test]
    fn rollback_restores_added_variables_relations_constants_and_representatives() {
        let mut union_find = populated(2);
        let initial = union_find.clone();
        let checkpoint = union_find.checkpoint();
        assert!(union_find.add_variable(variable(9)));
        union_find
            .relate(variable(9), &Rational::from(4), variable(1))
            .unwrap();
        union_find
            .relate(variable(0), &rational(3, 2), variable(9))
            .unwrap();
        union_find.assign(variable(9), &Rational::from(-8)).unwrap();
        let representative_before_rollback = union_find.representative(variable(0)).unwrap();
        assert_ne!(representative_before_rollback.0, variable(0));

        union_find.rollback(checkpoint);
        assert_eq!(union_find, initial);
        assert!(!union_find.contains(variable(9)));
        assert_eq!(
            union_find.representative(variable(0)).unwrap(),
            (variable(0), Rational::one())
        );
        assert_eq!(union_find.ratio(variable(0), variable(1)).unwrap(), None);
    }

    #[test]
    fn equal_size_union_uses_stable_flow_variable_tie_break() {
        let mut forward = populated(2);
        let mut reverse = populated(2);
        forward
            .relate(variable(1), &Rational::from(2), variable(0))
            .unwrap();
        reverse
            .relate(variable(0), &rational(1, 2), variable(1))
            .unwrap();

        assert_eq!(
            forward.representative(variable(1)).unwrap(),
            (variable(0), Rational::from(2))
        );
        assert_eq!(forward, reverse);
    }

    #[derive(Clone, Copy, Debug)]
    struct OracleRelation {
        left: FlowVarId,
        factor_numerator: i64,
        factor_denominator: i64,
        right: FlowVarId,
    }

    impl OracleRelation {
        fn factor(self) -> Rational {
            rational(self.factor_numerator, self.factor_denominator)
        }
    }

    #[derive(Clone, Debug, Default)]
    struct NaiveOracle {
        variables: BTreeSet<FlowVarId>,
        relations: Vec<OracleRelation>,
        assignments: Vec<(FlowVarId, Rational)>,
    }

    #[derive(Clone, Debug)]
    struct ComponentAnalysis {
        coefficients: BTreeMap<FlowVarId, Rational>,
        forced_zero: bool,
        known_start: Option<Rational>,
        consistent: bool,
    }

    impl NaiveOracle {
        fn populated(count: u32) -> Self {
            Self {
                variables: (0..count).map(variable).collect(),
                relations: Vec::new(),
                assignments: Vec::new(),
            }
        }

        fn analyze(&self, start: FlowVarId) -> ComponentAnalysis {
            let mut coefficients = BTreeMap::from([(start, Rational::one())]);
            let mut queue = VecDeque::from([start]);
            let mut forced_zero = false;
            while let Some(current) = queue.pop_front() {
                let current_coefficient = coefficients[&current].clone();
                for relation in &self.relations {
                    let factor = relation.factor();
                    let derived = if relation.left == current {
                        Some((relation.right, &current_coefficient / &factor))
                    } else if relation.right == current {
                        Some((relation.left, &factor * &current_coefficient))
                    } else {
                        None
                    };
                    let Some((neighbor, coefficient)) = derived else {
                        continue;
                    };
                    if let Some(existing) = coefficients.get(&neighbor) {
                        forced_zero |= existing != &coefficient;
                    } else {
                        coefficients.insert(neighbor, coefficient);
                        queue.push_back(neighbor);
                    }
                }
            }

            let mut known_start = forced_zero.then(Rational::zero);
            let mut consistent = true;
            for (variable, value) in &self.assignments {
                let Some(coefficient) = coefficients.get(variable) else {
                    continue;
                };
                let candidate = value / coefficient;
                if let Some(known) = &known_start {
                    consistent &= known == &candidate;
                } else {
                    known_start = Some(candidate);
                }
            }
            ComponentAnalysis {
                coefficients,
                forced_zero,
                known_start,
                consistent,
            }
        }

        fn is_consistent(&self) -> bool {
            self.variables
                .iter()
                .all(|variable| self.analyze(*variable).consistent)
        }

        fn try_relation(&mut self, relation: OracleRelation) -> bool {
            let mut candidate = self.clone();
            candidate.relations.push(relation);
            if candidate.is_consistent() {
                *self = candidate;
                true
            } else {
                false
            }
        }

        fn try_assignment(&mut self, variable: FlowVarId, value: Rational) -> bool {
            let mut candidate = self.clone();
            candidate.assignments.push((variable, value));
            if candidate.is_consistent() {
                *self = candidate;
                true
            } else {
                false
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Operation {
        Relate(OracleRelation),
        Assign(FlowVarId, i64),
    }

    const OPERATIONS: [Operation; 8] = [
        Operation::Relate(OracleRelation {
            left: FlowVarId(0),
            factor_numerator: 2,
            factor_denominator: 1,
            right: FlowVarId(1),
        }),
        Operation::Relate(OracleRelation {
            left: FlowVarId(1),
            factor_numerator: 3,
            factor_denominator: 1,
            right: FlowVarId(2),
        }),
        Operation::Relate(OracleRelation {
            left: FlowVarId(0),
            factor_numerator: 3,
            factor_denominator: 1,
            right: FlowVarId(2),
        }),
        Operation::Relate(OracleRelation {
            left: FlowVarId(2),
            factor_numerator: -1,
            factor_denominator: 1,
            right: FlowVarId(0),
        }),
        Operation::Relate(OracleRelation {
            left: FlowVarId(1),
            factor_numerator: 1,
            factor_denominator: 2,
            right: FlowVarId(0),
        }),
        Operation::Assign(FlowVarId(0), 0),
        Operation::Assign(FlowVarId(1), 1),
        Operation::Assign(FlowVarId(2), -2),
    ];

    fn apply_to_both(
        union_find: &mut WeightedUnionFind,
        oracle: &mut NaiveOracle,
        operation: Operation,
    ) {
        let (outcome, accepted) = match operation {
            Operation::Relate(relation) => (
                union_find
                    .relate(relation.left, &relation.factor(), relation.right)
                    .unwrap(),
                oracle.try_relation(relation),
            ),
            Operation::Assign(variable, value) => (
                union_find.assign(variable, &Rational::from(value)).unwrap(),
                oracle.try_assignment(variable, Rational::from(value)),
            ),
        };
        assert_eq!(outcome.is_consistent(), accepted, "{operation:?}");
    }

    fn assert_matches_oracle(union_find: &WeightedUnionFind, oracle: &NaiveOracle) {
        assert_eq!(union_find.len(), oracle.variables.len());
        for variable in &oracle.variables {
            let analysis = oracle.analyze(*variable);
            assert!(analysis.consistent);
            assert_eq!(
                union_find.known_value(*variable).unwrap(),
                analysis.known_start
            );
            for other in &oracle.variables {
                let stored_ratio = union_find.ratio(*variable, *other).unwrap();
                let other_analysis = oracle.analyze(*other);
                let oracle_ratio = other_analysis.coefficients.get(variable).cloned();
                assert_eq!(stored_ratio.is_some(), oracle_ratio.is_some());
                if !other_analysis.forced_zero {
                    assert_eq!(stored_ratio, oracle_ratio);
                }
            }
        }
    }

    #[test]
    fn exhaustive_small_sequences_match_naive_oracle_and_rollback_exactly() {
        let sequence_count = OPERATIONS.len().pow(4);
        for encoded_sequence in 0..sequence_count {
            let mut digits = encoded_sequence;
            let mut sequence = [OPERATIONS[0]; 4];
            for operation in &mut sequence {
                *operation = OPERATIONS[digits % OPERATIONS.len()];
                digits /= OPERATIONS.len();
            }

            let mut union_find = populated(3);
            let mut oracle = NaiveOracle::populated(3);
            for operation in &sequence[..2] {
                apply_to_both(&mut union_find, &mut oracle, *operation);
                assert_matches_oracle(&union_find, &oracle);
            }
            let checkpoint = union_find.checkpoint();
            let union_find_snapshot = union_find.clone();
            let oracle_snapshot = oracle.clone();
            for operation in &sequence[2..] {
                apply_to_both(&mut union_find, &mut oracle, *operation);
                assert_matches_oracle(&union_find, &oracle);
            }

            union_find.rollback(checkpoint);
            assert_eq!(union_find, union_find_snapshot);
            assert_matches_oracle(&union_find, &oracle_snapshot);
        }
    }

    #[test]
    fn dirty_bounds_follow_merged_components_and_rollback() {
        let mut union_find = populated(4);
        assert_eq!(union_find.dirty_variables().len(), 4);
        union_find.clear_bounds_dirty();
        assert!(!union_find.is_bounds_dirty());

        assert_eq!(
            union_find.relate(variable(0), &Rational::from(2), variable(1)),
            Ok(ConstraintOutcome::Changed)
        );
        let mut merged = union_find.dirty_variables();
        merged.sort_by_key(|variable| variable.0);
        assert_eq!(merged, vec![variable(0), variable(1)]);

        union_find.clear_bounds_dirty();
        let checkpoint = union_find.checkpoint();
        assert_eq!(
            union_find.assign(variable(2), &Rational::from(3)),
            Ok(ConstraintOutcome::Changed)
        );
        assert_eq!(union_find.dirty_variables(), vec![variable(2)]);
        union_find.rollback(checkpoint);
        assert!(!union_find.is_bounds_dirty());
        assert_eq!(union_find.dirty_variables(), Vec::<FlowVarId>::new());
    }
}
