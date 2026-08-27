//! Stable component-record encoding and verified high-level persistence.
//!
//! The database payload deliberately contains both the intrinsic exact contract
//! and the canonical physical witness. Import trusts neither: it reconstructs a
//! fresh component from the witness, re-encodes every derived field, and accepts
//! the row only when the canonical bytes match exactly. The symbolic catalog is
//! a conservative Pareto frontier. Every omitted record has a persisted exact
//! dominance path from a retained record, and the loader verifies every proof
//! before relying on that omission.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalNode, ProducerPortRef, Rational,
};
use solver_core::components::{
    BoundarySignature, CanonicalComponentLink, CanonicalComponentWitness, CapacityBehavior,
    CapacityTransparency, Component, ComponentCanonicalKey, ComponentDominanceProof,
    ComponentRecord, DomainContainmentProof, DominanceDecision, ExactMatrix, FeasibilityDomain,
    FrontierInsertion, GlobalComponentParetoFrontier, LinearImplicationCertificate,
    PeakDominanceProof, PeakRowProof, try_prove_global_dominance,
    verify_global_component_dominance,
};

use crate::{ComponentDatabase, DatabaseError, Decoder, DominanceRecord, Encoder, RawRecordId};

const COMPONENT_RECORD_MAGIC: &[u8] = b"satisfactory-exact-component";
const COMPONENT_RECORD_VERSION: u16 = 1;
const COMPONENT_BEHAVIOR_MAGIC: &[u8] = b"satisfactory-component-behavior";
const COMPONENT_BEHAVIOR_VERSION: u16 = 1;
const COMPONENT_PROOF_MAGIC: &[u8] = b"satisfactory-component-proof";
const COMPONENT_PROOF_VERSION: u16 = 1;
const DOMINANCE_PROOF_MAGIC: &[u8] = b"satisfactory-component-dominance";
const DOMINANCE_PROOF_VERSION: u16 = 1;
const MAX_SEQUENCE_ITEMS: usize = 1_000_000;

impl ComponentDatabase {
    /// Transactionally stores a canonical component record.
    ///
    /// The raw row is content-addressed by the full stable payload. The behavior
    /// key and proof summary are independently domain-separated by the `SQLite`
    /// layer.
    ///
    /// # Errors
    ///
    /// Returns a database error if the transactional content-addressed write fails.
    pub fn store_component(
        &self,
        component: &Component,
    ) -> Result<Option<RawRecordId>, DatabaseError> {
        let record = component.export_record();
        let behavior_payload = encode_component_behavior(component);
        let stored = self.store_component_blob(
            &behavior_payload,
            &encode_component_record(&record),
            &encode_component_proof(component),
        )?;
        let Some(id) = stored else {
            return Ok(None);
        };
        self.rebuild_symbolic_frontier(self.behavior_record_id(&behavior_payload))?;
        Ok(Some(id))
    }

    /// Loads one component only after raw hash checks and full exact physical
    /// reconstruction. Any malformed or semantically stale row is quarantined
    /// and behaves as an accelerator miss.
    ///
    /// # Errors
    ///
    /// Returns only operational database/quarantine failures. Invalid records
    /// themselves return `Ok(None)`.
    pub fn load_component(&self, id: RawRecordId) -> Result<Option<Component>, DatabaseError> {
        let Some(blob) = self.load_component_blob(id)? else {
            return Ok(None);
        };
        let component = match decode_component_record(&blob.payload) {
            Ok(component) => component,
            Err(reason) => {
                self.quarantine_component_blob(id, &format!("component codec: {reason}"))?;
                return Ok(None);
            }
        };
        if blob.behavior_id != self.behavior_record_id(&encode_component_behavior(&component))
            || blob.proof_payload != encode_component_proof(&component)
        {
            self.quarantine_component_blob(
                id,
                "component behavior identifier or proof summary mismatch",
            )?;
            return Ok(None);
        }
        Ok(Some(component))
    }

    /// Returns every compatible, independently reconstructed component in
    /// deterministic content-address order.
    ///
    /// # Errors
    ///
    /// Returns a database error if enumeration or quarantine fails.
    pub fn components(&self) -> Result<Vec<Component>, DatabaseError> {
        let ids = self
            .component_blobs()?
            .into_iter()
            .map(|blob| blob.id)
            .collect::<Vec<_>>();
        let mut components = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(component) = self.load_component(id)? {
                components.push(component);
            }
        }
        Ok(components)
    }

    /// Returns the conservative persistent symbolic Pareto frontier.
    ///
    /// The database membership table is only a hint. A component is omitted
    /// from this result only when a path of stored global dominance
    /// certificates starts at a retained member and reaches that component.
    /// Every certificate is decoded and verified against independently rebuilt
    /// endpoint components. Missing or invalid proof data retains the affected
    /// alternative, so corruption can reduce acceleration but cannot remove a
    /// capacity-relevant implementation.
    ///
    /// # Errors
    ///
    /// Returns an operational database or quarantine failure.
    pub fn frontier_components(&self) -> Result<Vec<Component>, DatabaseError> {
        let mut components = BTreeMap::<RawRecordId, Component>::new();
        for blob in self.component_blobs()? {
            if let Some(component) = self.load_component(blob.id)? {
                components.insert(blob.id, component);
            }
        }
        let members = self
            .symbolic_frontier_ids()?
            .into_iter()
            .filter(|id| components.contains_key(id))
            .collect::<BTreeSet<_>>();
        let mut edges = BTreeMap::<RawRecordId, Vec<RawRecordId>>::new();
        for record in self.dominance_records()? {
            let Some(dominator) = components.get(&record.dominator_id) else {
                continue;
            };
            let Some(dominated) = components.get(&record.dominated_id) else {
                continue;
            };
            let valid = decode_dominance_proof(&record.proof_payload)
                .is_ok_and(|proof| verify_global_component_dominance(&proof, dominator, dominated));
            if valid {
                edges
                    .entry(record.dominator_id)
                    .or_default()
                    .push(record.dominated_id);
            } else {
                self.quarantine_dominance_record(
                    record.dominator_id,
                    record.dominated_id,
                    "dominance codec or exact verification failed",
                )?;
            }
        }
        for dominated in edges.values_mut() {
            dominated.sort_unstable();
            dominated.dedup();
        }

        let mut proven_reachable = members.clone();
        let mut queue = members.iter().copied().collect::<VecDeque<_>>();
        while let Some(dominator) = queue.pop_front() {
            for &dominated in edges.get(&dominator).into_iter().flatten() {
                if proven_reachable.insert(dominated) {
                    queue.push_back(dominated);
                }
            }
        }

        let mut retained = components
            .into_iter()
            .filter(|(id, _)| members.contains(id) || !proven_reachable.contains(id))
            .collect::<Vec<_>>();
        retained.sort_by(|(left_id, left), (right_id, right)| {
            (left.canonical_key(), left_id).cmp(&(right.canonical_key(), right_id))
        });
        Ok(retained
            .into_iter()
            .map(|(_, component)| component)
            .collect())
    }

    fn rebuild_symbolic_frontier(&self, behavior_id: RawRecordId) -> Result<(), DatabaseError> {
        let mut candidates = Vec::new();
        for id in self.component_ids_for_behavior(behavior_id)? {
            if let Some(component) = self.load_component(id)? {
                candidates.push((id, component));
            }
        }
        candidates.sort_by(|(left_id, left), (right_id, right)| {
            (left.canonical_key(), left_id).cmp(&(right.canonical_key(), right_id))
        });

        let mut frontier = GlobalComponentParetoFrontier::new();
        let mut retained = BTreeMap::<ComponentCanonicalKey, Vec<(RawRecordId, Component)>>::new();
        let mut dominance = Vec::new();
        for (id, component) in candidates {
            match frontier.insert(component.clone()) {
                FrontierInsertion::Duplicate => {
                    // A duplicate canonical key is exact equivalence, but there
                    // is no separate persisted equivalence certificate. Keep
                    // every such content record rather than relying on an
                    // unstored assertion.
                    retained
                        .entry(component.canonical_key().clone())
                        .or_default()
                        .push((id, component));
                }
                FrontierInsertion::Dominated { by, proof } => {
                    let Some(dominator_id) = retained
                        .get(by.as_ref())
                        .and_then(|records| records.first())
                        .map(|(id, _)| *id)
                    else {
                        return Err(DatabaseError::InvalidDominanceProof);
                    };
                    if !verify_global_component_dominance(
                        &proof,
                        &retained[by.as_ref()][0].1,
                        &component,
                    ) {
                        return Err(DatabaseError::InvalidDominanceProof);
                    }
                    dominance.push(DominanceRecord {
                        dominator_id,
                        dominated_id: id,
                        proof_payload: encode_dominance_proof(&proof),
                    });
                }
                FrontierInsertion::Retained { removed } => {
                    for removed_key in removed {
                        let Some(removed_records) = retained.remove(&removed_key) else {
                            return Err(DatabaseError::InvalidDominanceProof);
                        };
                        for (dominated_id, dominated) in removed_records {
                            let DominanceDecision::Proven(proof) =
                                try_prove_global_dominance(&component, &dominated)
                            else {
                                return Err(DatabaseError::InvalidDominanceProof);
                            };
                            if !verify_global_component_dominance(&proof, &component, &dominated) {
                                return Err(DatabaseError::InvalidDominanceProof);
                            }
                            dominance.push(DominanceRecord {
                                dominator_id: id,
                                dominated_id,
                                proof_payload: encode_dominance_proof(&proof),
                            });
                        }
                    }
                    retained.insert(component.canonical_key().clone(), vec![(id, component)]);
                }
            }
        }
        let retained_ids = retained
            .into_values()
            .flatten()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        self.replace_symbolic_frontier(behavior_id, &retained_ids, &dominance)
    }
}

/// Stable exact component payload used for content addressing and persistence.
#[must_use]
pub fn encode_component_record(record: &ComponentRecord) -> Vec<u8> {
    let mut derived = Encoder::new();
    encode_boundary(&mut derived, record.behavior_key.boundary());
    encode_matrix(&mut derived, record.behavior_key.transfer());
    encode_canonical_key(&mut derived, &record.canonical_key);
    encode_boundary(&mut derived, &record.boundary);
    encode_matrix(&mut derived, &record.transfer);
    encode_matrix(&mut derived, &record.internal_flow_map);
    encode_domain(&mut derived, &record.domain);
    encode_capacity(&mut derived, &record.capacity);
    encode_profile(&mut derived, record.profile);
    derived.u32(record.internal_links);

    let mut witness = Encoder::new();
    encode_witness(&mut witness, &record.canonical_witness);

    let mut payload = Encoder::new();
    payload.bytes(COMPONENT_RECORD_MAGIC);
    payload.u16(COMPONENT_RECORD_VERSION);
    payload.bytes(&derived.finish());
    payload.bytes(&witness.finish());
    payload.finish()
}

fn decode_component_record(payload: &[u8]) -> Result<Component, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, COMPONENT_RECORD_MAGIC)?;
    require_version(&mut decoder, COMPONENT_RECORD_VERSION)?;
    let _derived = decoder.bytes().map_err(display)?.to_vec();
    let witness_payload = decoder.bytes().map_err(display)?.to_vec();
    decoder.finish().map_err(display)?;

    let witness = decode_witness(&witness_payload)?;
    let component =
        Component::import_canonical_witness(&witness).map_err(|error| error.to_string())?;
    if encode_component_record(&component.export_record()) != payload {
        return Err("derived exact fields disagree with the reconstructed witness".to_owned());
    }
    Ok(component)
}

fn encode_component_behavior(component: &Component) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(COMPONENT_BEHAVIOR_MAGIC);
    encoder.u16(COMPONENT_BEHAVIOR_VERSION);
    encode_boundary(&mut encoder, component.behavior_key().boundary());
    encode_matrix(&mut encoder, component.behavior_key().transfer());
    encoder.finish()
}

fn encode_component_proof(component: &Component) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(COMPONENT_PROOF_MAGIC);
    encoder.u16(COMPONENT_PROOF_VERSION);
    encode_profile(&mut encoder, component.profile());
    encoder.u32(component.internal_link_count());
    encoder.bytes(component.canonical_key().projection_bytes());
    encoder.finish()
}

fn encode_dominance_proof(proof: &ComponentDominanceProof) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(DOMINANCE_PROOF_MAGIC);
    encoder.u16(DOMINANCE_PROOF_VERSION);
    encode_len(&mut encoder, proof.dominating_input_order.len());
    for &index in &proof.dominating_input_order {
        encode_len(&mut encoder, index);
    }
    encode_len(&mut encoder, proof.dominating_output_order.len());
    for &index in &proof.dominating_output_order {
        encode_len(&mut encoder, index);
    }
    encode_len(&mut encoder, proof.domain.equality_proofs.len());
    for implication in &proof.domain.equality_proofs {
        encode_implication(&mut encoder, implication);
    }
    encode_len(&mut encoder, proof.domain.strict_proofs.len());
    for implication in &proof.domain.strict_proofs {
        encode_implication(&mut encoder, implication);
    }
    encode_len(&mut encoder, proof.peak.rows.len());
    for row in &proof.peak.rows {
        encode_len(&mut encoder, row.right_row);
        encode_implication(&mut encoder, &row.implication);
    }
    encoder.finish()
}

fn decode_dominance_proof(payload: &[u8]) -> Result<ComponentDominanceProof, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, DOMINANCE_PROOF_MAGIC)?;
    require_version(&mut decoder, DOMINANCE_PROOF_VERSION)?;
    let input_count = decode_len(&mut decoder)?;
    let dominating_input_order = (0..input_count)
        .map(|_| decode_len(&mut decoder))
        .collect::<Result<Vec<_>, _>>()?;
    let output_count = decode_len(&mut decoder)?;
    let dominating_output_order = (0..output_count)
        .map(|_| decode_len(&mut decoder))
        .collect::<Result<Vec<_>, _>>()?;
    let equality_count = decode_len(&mut decoder)?;
    let equality_proofs = (0..equality_count)
        .map(|_| decode_implication(&mut decoder))
        .collect::<Result<Vec<_>, _>>()?;
    let strict_count = decode_len(&mut decoder)?;
    let strict_proofs = (0..strict_count)
        .map(|_| decode_implication(&mut decoder))
        .collect::<Result<Vec<_>, _>>()?;
    let peak_count = decode_len(&mut decoder)?;
    let rows = (0..peak_count)
        .map(|_| {
            Ok(PeakRowProof {
                right_row: decode_len(&mut decoder)?,
                implication: decode_implication(&mut decoder)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    decoder.finish().map_err(display)?;
    Ok(ComponentDominanceProof {
        dominating_input_order,
        dominating_output_order,
        domain: DomainContainmentProof {
            equality_proofs,
            strict_proofs,
        },
        peak: PeakDominanceProof { rows },
    })
}

fn decode_implication(decoder: &mut Decoder<'_>) -> Result<LinearImplicationCertificate, String> {
    Ok(LinearImplicationCertificate {
        strict_multipliers: decode_rationals(decoder)?,
        equality_multipliers: decode_rationals(decoder)?,
    })
}

pub(crate) fn encode_canonical_key(encoder: &mut Encoder, key: &ComponentCanonicalKey) {
    encode_boundary(encoder, key.boundary());
    encode_matrix(encoder, key.transfer());
    encode_domain(encoder, key.domain());
    encode_matrix(encoder, key.internal_flow_map());
    encode_profile(encoder, key.profile());
    encoder.u32(key.internal_link_count());
    encoder.bytes(key.projection_bytes());
}

pub(crate) fn encode_boundary(encoder: &mut Encoder, boundary: &BoundarySignature) {
    encode_len(encoder, boundary.input_count());
    encode_len(encoder, boundary.output_count());
    encode_len(encoder, boundary.feedback_relation().len());
    for &forbidden in boundary.feedback_relation() {
        encoder.bool(forbidden);
    }
}

fn encode_matrix(encoder: &mut Encoder, matrix: &ExactMatrix) {
    encode_len(encoder, matrix.column_count());
    encode_len(encoder, matrix.row_count());
    for row in matrix.rows() {
        for value in row {
            encoder.rational(value);
        }
    }
}

fn encode_domain(encoder: &mut Encoder, domain: &FeasibilityDomain) {
    encode_matrix(encoder, domain.equalities());
    encode_matrix(encoder, domain.strict_rows());
    encoder.bool(domain.is_proven_empty());
}

fn encode_capacity(encoder: &mut Encoder, capacity: &CapacityBehavior) {
    encode_matrix(encoder, capacity.internal_rows());
    encode_matrix(encoder, capacity.boundary_rows());
    match capacity.transparency() {
        CapacityTransparency::Proven(proof) => {
            encoder.u8(0);
            encode_len(encoder, proof.rows.len());
            for row in &proof.rows {
                encode_len(encoder, row.boundary_row);
                encode_implication(encoder, &row.implication);
            }
        }
        CapacityTransparency::Disproven {
            input,
            internal_peak,
            boundary_peak,
        } => {
            encoder.u8(1);
            encode_rationals(encoder, input);
            encoder.rational(internal_peak);
            encoder.rational(boundary_peak);
        }
        CapacityTransparency::Unknown => encoder.u8(2),
    }
}

fn encode_implication(encoder: &mut Encoder, proof: &LinearImplicationCertificate) {
    encode_rationals(encoder, &proof.strict_multipliers);
    encode_rationals(encoder, &proof.equality_multipliers);
}

fn encode_rationals(encoder: &mut Encoder, values: &[Rational]) {
    encode_len(encoder, values.len());
    for value in values {
        encoder.rational(value);
    }
}

pub(crate) fn encode_profile(encoder: &mut Encoder, profile: NodeProfile) {
    encoder.u32(profile.splitter2);
    encoder.u32(profile.splitter3);
    encoder.u32(profile.merger2);
    encoder.u32(profile.merger3);
}

fn encode_witness(encoder: &mut Encoder, witness: &CanonicalComponentWitness) {
    encode_len(encoder, witness.nodes.len());
    for node in &witness.nodes {
        encoder.u32(node.id.0);
        encoder.u8(node_type_tag(node.node_type));
    }
    encode_len(encoder, witness.internal_links.len());
    for link in &witness.internal_links {
        encode_producer(encoder, link.producer);
        encode_consumer(encoder, link.consumer);
        encode_rationals(encoder, &link.coefficients);
    }
    encode_len(encoder, witness.boundary_inputs.len());
    for &input in &witness.boundary_inputs {
        encode_consumer(encoder, input);
    }
    encode_len(encoder, witness.boundary_outputs.len());
    for &output in &witness.boundary_outputs {
        encode_producer(encoder, output);
    }
}

fn decode_witness(payload: &[u8]) -> Result<CanonicalComponentWitness, String> {
    let mut decoder = Decoder::new(payload);
    let node_count = decode_len(&mut decoder)?;
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        nodes.push(PhysicalNode {
            id: NodeId(decoder.u32().map_err(display)?),
            node_type: decode_node_type(decoder.u8().map_err(display)?)?,
        });
    }
    let link_count = decode_len(&mut decoder)?;
    let mut internal_links = Vec::with_capacity(link_count);
    for _ in 0..link_count {
        internal_links.push(CanonicalComponentLink {
            producer: decode_producer(&mut decoder)?,
            consumer: decode_consumer(&mut decoder)?,
            coefficients: decode_rationals(&mut decoder)?,
        });
    }
    let input_count = decode_len(&mut decoder)?;
    let mut boundary_inputs = Vec::with_capacity(input_count);
    for _ in 0..input_count {
        boundary_inputs.push(decode_consumer(&mut decoder)?);
    }
    let output_count = decode_len(&mut decoder)?;
    let mut boundary_outputs = Vec::with_capacity(output_count);
    for _ in 0..output_count {
        boundary_outputs.push(decode_producer(&mut decoder)?);
    }
    decoder.finish().map_err(display)?;
    Ok(CanonicalComponentWitness {
        nodes,
        internal_links,
        boundary_inputs,
        boundary_outputs,
    })
}

fn encode_producer(encoder: &mut Encoder, reference: ProducerPortRef) {
    match reference {
        ProducerPortRef::Input(index) => {
            encoder.u8(0);
            encoder.u32(index.0);
        }
        ProducerPortRef::Node { node, port } => {
            encoder.u8(1);
            encoder.u32(node.0);
            encoder.u8(port);
        }
    }
}

fn decode_producer(decoder: &mut Decoder<'_>) -> Result<ProducerPortRef, String> {
    match decoder.u8().map_err(display)? {
        0 => Ok(ProducerPortRef::Input(InputTerminalIndex(
            decoder.u32().map_err(display)?,
        ))),
        1 => Ok(ProducerPortRef::Node {
            node: NodeId(decoder.u32().map_err(display)?),
            port: decoder.u8().map_err(display)?,
        }),
        tag => Err(format!("invalid producer endpoint tag {tag}")),
    }
}

fn encode_consumer(encoder: &mut Encoder, reference: ConsumerPortRef) {
    match reference {
        ConsumerPortRef::Output(index) => {
            encoder.u8(0);
            encoder.u32(index.0);
        }
        ConsumerPortRef::Node { node, port } => {
            encoder.u8(1);
            encoder.u32(node.0);
            encoder.u8(port);
        }
        ConsumerPortRef::Discard(index) => {
            encoder.u8(2);
            encoder.u32(index.0);
        }
    }
}

fn decode_consumer(decoder: &mut Decoder<'_>) -> Result<ConsumerPortRef, String> {
    match decoder.u8().map_err(display)? {
        0 => Ok(ConsumerPortRef::Output(OutputTerminalIndex(
            decoder.u32().map_err(display)?,
        ))),
        1 => Ok(ConsumerPortRef::Node {
            node: NodeId(decoder.u32().map_err(display)?),
            port: decoder.u8().map_err(display)?,
        }),
        2 => Ok(ConsumerPortRef::Discard(DiscardTerminalIndex(
            decoder.u32().map_err(display)?,
        ))),
        tag => Err(format!("invalid consumer endpoint tag {tag}")),
    }
}

const fn node_type_tag(node_type: NodeType) -> u8 {
    match node_type {
        NodeType::Splitter2 => 0,
        NodeType::Splitter3 => 1,
        NodeType::Merger2 => 2,
        NodeType::Merger3 => 3,
    }
}

fn decode_node_type(tag: u8) -> Result<NodeType, String> {
    match tag {
        0 => Ok(NodeType::Splitter2),
        1 => Ok(NodeType::Splitter3),
        2 => Ok(NodeType::Merger2),
        3 => Ok(NodeType::Merger3),
        _ => Err(format!("invalid node type tag {tag}")),
    }
}

fn encode_len(encoder: &mut Encoder, length: usize) {
    encoder.u32(u32::try_from(length).expect("canonical sequence length must fit u32"));
}

fn decode_len(decoder: &mut Decoder<'_>) -> Result<usize, String> {
    let value = usize::try_from(decoder.u32().map_err(display)?)
        .map_err(|_| "sequence length cannot fit this platform".to_owned())?;
    if value > MAX_SEQUENCE_ITEMS {
        return Err("persistent sequence exceeds the item limit".to_owned());
    }
    Ok(value)
}

fn decode_rationals(decoder: &mut Decoder<'_>) -> Result<Vec<Rational>, String> {
    let count = decode_len(decoder)?;
    (0..count)
        .map(|_| decoder.rational().map_err(display))
        .collect()
}

fn require_bytes(decoder: &mut Decoder<'_>, expected: &[u8]) -> Result<(), String> {
    let actual = decoder.bytes().map_err(display)?;
    if actual == expected {
        Ok(())
    } else {
        Err("persistent record magic does not match".to_owned())
    }
}

fn require_version(decoder: &mut Decoder<'_>, expected: u16) -> Result<(), String> {
    let actual = decoder.u16().map_err(display)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "unsupported persistent record version {actual}, expected {expected}"
        ))
    }
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use solver_api::NodeProfile;
    use solver_reference::{
        CanonicalComponentTopology, ComponentCell, ComponentCellEnumeration,
        enumerate_component_cell,
    };
    use tempfile::tempdir;

    use super::*;

    fn splitter_component() -> Component {
        Component::import_canonical_witness(&CanonicalComponentWitness {
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            internal_links: Vec::new(),
            boundary_inputs: vec![ConsumerPortRef::Node {
                node: NodeId(0),
                port: 0,
            }],
            boundary_outputs: vec![
                ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
                ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 1,
                },
            ],
        })
        .unwrap()
    }

    fn component_from_parts(
        nodes: Vec<PhysicalNode>,
        internal_links: Vec<(ProducerPortRef, ConsumerPortRef)>,
        boundary_input: ConsumerPortRef,
        boundary_output: ProducerPortRef,
    ) -> Component {
        Component::import_canonical_witness(&CanonicalComponentWitness {
            nodes,
            internal_links: internal_links
                .into_iter()
                .map(|(producer, consumer)| CanonicalComponentLink {
                    producer,
                    consumer,
                    coefficients: Vec::new(),
                })
                .collect(),
            boundary_inputs: vec![boundary_input],
            boundary_outputs: vec![boundary_output],
        })
        .unwrap()
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

    fn two_node_identity(feedback: bool) -> Component {
        let nodes = vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)];
        if feedback {
            component_from_parts(
                nodes,
                vec![
                    (producer(1, 0), consumer(0, 0)),
                    (producer(0, 0), consumer(1, 0)),
                ],
                consumer(1, 1),
                producer(0, 1),
            )
        } else {
            component_from_parts(
                nodes,
                vec![
                    (producer(0, 0), consumer(1, 0)),
                    (producer(0, 1), consumer(1, 1)),
                ],
                consumer(0, 0),
                producer(1, 0),
            )
        }
    }

    fn four_node_identity() -> Component {
        component_from_parts(
            vec![
                node(0, NodeType::Splitter2),
                node(1, NodeType::Merger2),
                node(2, NodeType::Splitter2),
                node(3, NodeType::Merger2),
            ],
            vec![
                (producer(0, 0), consumer(1, 0)),
                (producer(0, 1), consumer(1, 1)),
                (producer(1, 0), consumer(2, 0)),
                (producer(2, 0), consumer(3, 0)),
                (producer(2, 1), consumer(3, 1)),
            ],
            consumer(0, 0),
            producer(3, 0),
        )
    }

    fn component_from_reference(topology: &CanonicalComponentTopology) -> Option<Component> {
        Component::import_canonical_witness(&CanonicalComponentWitness {
            nodes: topology.nodes.clone(),
            internal_links: topology
                .internal_links
                .iter()
                .map(|link| CanonicalComponentLink {
                    producer: link.producer,
                    consumer: link.consumer,
                    coefficients: Vec::new(),
                })
                .collect(),
            boundary_inputs: topology.boundary_inputs.clone(),
            boundary_outputs: topology.boundary_outputs.clone(),
        })
        .ok()
    }

    #[test]
    fn stable_record_round_trip_reconstructs_every_exact_field() {
        let component = splitter_component();
        let payload = encode_component_record(&component.export_record());
        let rebuilt = decode_component_record(&payload).unwrap();
        assert_eq!(rebuilt.export_record(), component.export_record());
        assert_eq!(encode_component_record(&rebuilt.export_record()), payload,);
    }

    #[test]
    fn derived_or_witness_tampering_is_rejected_and_quarantined() {
        let component = splitter_component();
        let database = ComponentDatabase::open(&crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::InMemory,
            ..crate::ComponentDatabaseConfig::default()
        })
        .unwrap();
        let id = database.store_component(&component).unwrap().unwrap();
        assert_eq!(
            database
                .load_component(id)
                .unwrap()
                .unwrap()
                .export_record(),
            component.export_record(),
        );

        let mut tampered = encode_component_record(&component.export_record());
        let derived_length_offset = 4 + COMPONENT_RECORD_MAGIC.len() + 2;
        let derived_length = u32::from_be_bytes(
            tampered[derived_length_offset..derived_length_offset + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let last_derived_byte = derived_length_offset + 4 + derived_length - 1;
        tampered[last_derived_byte] ^= 1;
        let tampered_id = database
            .store_component_blob(
                &encode_component_behavior(&component),
                &tampered,
                &encode_component_proof(&component),
            )
            .unwrap()
            .unwrap();
        assert_eq!(database.load_component(tampered_id).unwrap(), None);
        assert_eq!(database.status().unwrap().quarantined_records, 1);
    }

    #[test]
    fn persistent_frontier_reopens_with_reverified_peak_dominance() {
        let directory = tempdir().unwrap();
        let config = crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::Path(directory.path().join("frontier.sqlite3")),
            ..crate::ComponentDatabaseConfig::default()
        };
        let acyclic = two_node_identity(false);
        let feedback = two_node_identity(true);
        let database = ComponentDatabase::open(&config).unwrap();
        database.store_component(&feedback).unwrap();
        database.store_component(&acyclic).unwrap();
        assert_eq!(database.components().unwrap().len(), 2);
        assert_eq!(
            database.frontier_components().unwrap(),
            vec![acyclic.clone()]
        );
        assert_eq!(database.dominance_records().unwrap().len(), 1);
        drop(database);

        let reopened = ComponentDatabase::open(&config).unwrap();
        assert_eq!(reopened.frontier_components().unwrap(), vec![acyclic]);
        assert_eq!(reopened.status().unwrap().quarantined_records, 0);
        let expected_dominance = reopened.dominance_records().unwrap();

        let reverse = ComponentDatabase::open(&crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::InMemory,
            ..crate::ComponentDatabaseConfig::default()
        })
        .unwrap();
        reverse.store_component(&two_node_identity(false)).unwrap();
        reverse.store_component(&feedback).unwrap();
        assert_eq!(
            reverse.frontier_components().unwrap(),
            reopened.frontier_components().unwrap()
        );
        assert_eq!(reverse.dominance_records().unwrap(), expected_dominance);
    }

    #[test]
    fn cheaper_high_peak_and_costlier_low_peak_capacity_alternatives_both_survive() {
        let cheap_high_peak = two_node_identity(true);
        let costly_low_peak = four_node_identity();
        assert!(cheap_high_peak.cost() < costly_low_peak.cost());
        assert!(
            cheap_high_peak
                .evaluate(&[Rational::one()], &Rational::one())
                .is_err()
        );
        assert!(
            costly_low_peak
                .evaluate(&[Rational::one()], &Rational::one())
                .is_ok()
        );
        assert!(matches!(
            try_prove_global_dominance(&cheap_high_peak, &costly_low_peak),
            DominanceDecision::Unknown(_)
        ));
        assert!(matches!(
            try_prove_global_dominance(&costly_low_peak, &cheap_high_peak),
            DominanceDecision::Unknown(_)
        ));

        let database = ComponentDatabase::open(&crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::InMemory,
            ..crate::ComponentDatabaseConfig::default()
        })
        .unwrap();
        database.store_component(&cheap_high_peak).unwrap();
        database.store_component(&costly_low_peak).unwrap();
        let frontier = database.frontier_components().unwrap();
        assert_eq!(frontier.len(), 2);
        let keys = frontier
            .iter()
            .map(Component::canonical_key)
            .collect::<BTreeSet<_>>();
        assert!(keys.contains(cheap_high_peak.canonical_key()));
        assert!(keys.contains(costly_low_peak.canonical_key()));
    }

    #[test]
    fn corrupt_dominance_certificate_restores_the_omitted_alternative() {
        let directory = tempdir().unwrap();
        let config = crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::Path(directory.path().join("corrupt-frontier.sqlite3")),
            ..crate::ComponentDatabaseConfig::default()
        };
        let database = ComponentDatabase::open(&config).unwrap();
        database.store_component(&two_node_identity(true)).unwrap();
        database.store_component(&two_node_identity(false)).unwrap();
        assert_eq!(database.frontier_components().unwrap().len(), 1);
        {
            let connection = database.connection.as_ref().unwrap().lock().unwrap();
            connection
                .execute(
                    "UPDATE dominance_edges SET proof_payload=X'00' WHERE fingerprint=?1",
                    [database.fingerprint().as_bytes().as_slice()],
                )
                .unwrap();
        }
        assert_eq!(database.frontier_components().unwrap().len(), 2);
        assert_eq!(database.status().unwrap().quarantined_records, 1);
        drop(database);

        let reopened = ComponentDatabase::open(&config).unwrap();
        assert_eq!(reopened.frontier_components().unwrap().len(), 2);
    }

    #[test]
    fn sqlite_frontier_matches_the_independent_reference_component_universe() {
        let cell = ComponentCell::new(
            NodeProfile {
                splitter2: 1,
                splitter3: 0,
                merger2: 1,
                merger3: 0,
            },
            1,
            1,
        )
        .unwrap();
        let ComponentCellEnumeration::Complete { topologies, .. } =
            enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap()
        else {
            panic!("an unset cancellation flag must exhaust the reference cell");
        };
        let mut candidates = topologies
            .iter()
            .filter_map(component_from_reference)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.canonical_key().cmp(right.canonical_key()));
        candidates.dedup_by(|left, right| left.canonical_key() == right.canonical_key());
        assert!(candidates.len() > 1);

        let mut expected = GlobalComponentParetoFrontier::new();
        for candidate in &candidates {
            expected.insert(candidate.clone());
        }
        let expected = expected
            .iter()
            .map(|component| component.canonical_key().clone())
            .collect::<Vec<_>>();

        let database = ComponentDatabase::open(&crate::ComponentDatabaseConfig {
            mode: crate::DatabaseMode::InMemory,
            ..crate::ComponentDatabaseConfig::default()
        })
        .unwrap();
        for candidate in candidates.into_iter().rev() {
            database.store_component(&candidate).unwrap();
        }
        let actual = database
            .frontier_components()
            .unwrap()
            .into_iter()
            .map(|component| component.canonical_key().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        for record in database.dominance_records().unwrap() {
            let dominator = database
                .load_component(record.dominator_id)
                .unwrap()
                .unwrap();
            let dominated = database
                .load_component(record.dominated_id)
                .unwrap()
                .unwrap();
            let proof = decode_dominance_proof(&record.proof_payload).unwrap();
            assert!(verify_global_component_dominance(
                &proof, &dominator, &dominated
            ));
        }
    }
}
