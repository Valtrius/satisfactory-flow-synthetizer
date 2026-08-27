//! Stable concrete-application records.
//!
//! Persisted application ledgers are checked for internal consistency, but are
//! never a completeness dependency. A loaded row supplies an independently
//! reconstructed feasible incumbent. The current process must re-establish any
//! optimality claim before it uses that claim to discharge search work.

use solver_api::{NodeProfile, Rational};
use solver_core::{
    component_application::{
        ApplicationOptimalityProof, ComponentApplicationKey, ComponentApplicationMatch,
        ComponentProfileProof, match_component_application,
    },
    component_optimizer::{ProvenApplicationComponent, verify_application_optimality_manifest},
    components::{Component, ComponentCost},
    work_table::{ComponentProvider, ComponentRepository},
};

use crate::{
    ComponentDatabase, DatabaseError, Decoder, Encoder, RawRecordId,
    records::{encode_boundary, encode_canonical_key, encode_profile},
};

const APPLICATION_KEY_MAGIC: &[u8] = b"satisfactory-component-application-key";
const APPLICATION_KEY_VERSION: u16 = 1;
const APPLICATION_PROOF_MAGIC: &[u8] = b"satisfactory-component-application-proof";
const APPLICATION_PROOF_VERSION: u16 = 1;
const MAX_PROFILE_OBLIGATIONS: usize = 1_000_000;

/// A persisted, exactly feasible application incumbent.
///
/// `stored_proof_summary` is diagnostic/re-verifiable metadata. Loading this
/// value does not by itself authorize pruning primitive search or claiming
/// application optimality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredApplicationCandidate {
    pub component: Component,
    pub application: ComponentApplicationMatch,
    pub stored_proof_summary: ApplicationOptimalityProof,
}

impl ComponentDatabase {
    /// Stores one in-process proven concrete application and its exact component.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError::InvalidApplicationProof`] if the value is not
    /// internally consistent, or an operational/content-address database error.
    pub fn store_proven_application(
        &self,
        proven: &ProvenApplicationComponent,
    ) -> Result<Option<RawRecordId>, DatabaseError> {
        let recomputed = match_component_application(&proven.component, &proven.proof.key)
            .map_err(|_| DatabaseError::InvalidApplicationProof)?;
        if recomputed.as_ref() != Some(&proven.application)
            || !verify_application_optimality_manifest(&proven.proof, &proven.component)
        {
            return Err(DatabaseError::InvalidApplicationProof);
        }
        let Some(component_id) = self.store_component(&proven.component)? else {
            return Ok(None);
        };
        self.store_application(
            &encode_application_key(&proven.proof.key),
            component_id,
            proven.proof.cost.nodes,
            proven.proof.cost.internal_links,
            &encode_application_proof(&proven.proof),
        )
    }

    /// Loads an independently reconstructed feasible incumbent for one exact
    /// common-scale application key.
    ///
    /// Any malformed component, scope mismatch, cost mismatch, or invalid proof
    /// summary is quarantined and returned as a miss. The returned summary still
    /// must not replace current-process exhaustive proof work.
    ///
    /// # Errors
    ///
    /// Returns only operational database/quarantine errors.
    pub fn lookup_application_candidate(
        &self,
        key: &ComponentApplicationKey,
    ) -> Result<Option<StoredApplicationCandidate>, DatabaseError> {
        let key_payload = encode_application_key(key);
        let Some(raw) = self.lookup_application(&key_payload)? else {
            return Ok(None);
        };
        let Some(component) = self.load_component(raw.winner_id)? else {
            self.quarantine_application_record(raw.id, "application winner component is invalid")?;
            return Ok(None);
        };
        let cost = component.cost();
        if raw.node_count != cost.nodes || raw.internal_link_count != cost.internal_links {
            self.quarantine_application_record(raw.id, "application cost disagrees with winner")?;
            return Ok(None);
        }
        let proof = match decode_application_proof(&raw.proof_payload, key, &component) {
            Ok(proof) if verify_application_optimality_manifest(&proof, &component) => proof,
            Ok(_) | Err(_) => {
                self.quarantine_application_record(
                    raw.id,
                    "application proof summary failed exact verification",
                )?;
                return Ok(None);
            }
        };
        let Ok(Some(application)) = match_component_application(&component, key) else {
            self.quarantine_application_record(
                raw.id,
                "application winner does not realize the indexed exact key",
            )?;
            return Ok(None);
        };
        Ok(Some(StoredApplicationCandidate {
            component,
            application,
            stored_proof_summary: proof,
        }))
    }
}

impl ComponentRepository<ComponentApplicationKey, ProvenApplicationComponent>
    for ComponentDatabase
{
    type Error = DatabaseError;

    fn load_complete(
        &self,
        key: &ComponentApplicationKey,
    ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
        Ok(self
            .lookup_application_candidate(key)?
            .map(|candidate| ProvenApplicationComponent {
                component: candidate.component,
                application: candidate.application,
                // This persisted summary is reconstructed and internally
                // verified, but the live runtime deliberately uses the whole
                // value only as an incumbent and redoes the finite proof.
                proof: candidate.stored_proof_summary,
            }))
    }

    fn store_complete(
        &self,
        key: &ComponentApplicationKey,
        value: &ProvenApplicationComponent,
    ) -> Result<(), Self::Error> {
        if &value.proof.key != key {
            return Err(DatabaseError::InvalidApplicationProof);
        }
        self.store_proven_application(value).map(|_| ())
    }
}

impl ComponentProvider<ComponentApplicationKey, ProvenApplicationComponent> for ComponentDatabase {
    type Error = DatabaseError;

    fn provide(
        &self,
        key: &ComponentApplicationKey,
    ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
        ComponentRepository::load_complete(self, key)
    }
}

/// Encodes the exact scale-invariant application identity used by the concrete index.
#[must_use]
pub fn encode_application_key(key: &ComponentApplicationKey) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(APPLICATION_KEY_MAGIC);
    encoder.u16(APPLICATION_KEY_VERSION);
    encode_boundary(&mut encoder, key.boundary());
    encode_rationals(&mut encoder, key.inputs());
    encode_rationals(&mut encoder, key.outputs());
    encoder.rational(key.max_link_rate());
    encoder.finish()
}

fn encode_application_proof(proof: &ApplicationOptimalityProof) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(APPLICATION_PROOF_MAGIC);
    encoder.u16(APPLICATION_PROOF_VERSION);
    encoder.bytes(&encode_application_key(&proof.key));
    let mut winner = Encoder::new();
    encode_canonical_key(&mut winner, &proof.winner);
    encoder.bytes(&winner.finish());
    encoder.u32(proof.cost.nodes);
    encoder.u32(proof.cost.internal_links);
    encode_len(&mut encoder, proof.exhausted_profiles.len());
    for profile in &proof.exhausted_profiles {
        encoder.u32(profile.node_count);
        encoder.u32(profile.internal_link_count);
        encode_profile(&mut encoder, profile.profile);
        encoder.u64(profile.physical_bijections_checked);
        encoder.u64(profile.canonical_topologies_checked);
    }
    encoder.finish()
}

fn decode_application_proof(
    payload: &[u8],
    expected_key: &ComponentApplicationKey,
    winner: &Component,
) -> Result<ApplicationOptimalityProof, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, APPLICATION_PROOF_MAGIC)?;
    require_version(&mut decoder, APPLICATION_PROOF_VERSION)?;
    if decoder.bytes().map_err(display)? != encode_application_key(expected_key) {
        return Err("application proof key does not match the concrete index".to_owned());
    }
    let mut expected_winner = Encoder::new();
    encode_canonical_key(&mut expected_winner, winner.canonical_key());
    if decoder.bytes().map_err(display)? != expected_winner.finish() {
        return Err("application proof winner identity does not match its component".to_owned());
    }
    let cost = ComponentCost {
        nodes: decoder.u32().map_err(display)?,
        internal_links: decoder.u32().map_err(display)?,
    };
    let count = decode_len(&mut decoder)?;
    let mut exhausted_profiles = Vec::with_capacity(count);
    for _ in 0..count {
        exhausted_profiles.push(ComponentProfileProof {
            node_count: decoder.u32().map_err(display)?,
            internal_link_count: decoder.u32().map_err(display)?,
            profile: decode_profile(&mut decoder)?,
            physical_bijections_checked: decoder.u64().map_err(display)?,
            canonical_topologies_checked: decoder.u64().map_err(display)?,
        });
    }
    decoder.finish().map_err(display)?;
    Ok(ApplicationOptimalityProof {
        key: expected_key.clone(),
        winner: winner.canonical_key().clone(),
        cost,
        exhausted_profiles,
    })
}

fn encode_rationals(encoder: &mut Encoder, values: &[Rational]) {
    encode_len(encoder, values.len());
    for value in values {
        encoder.rational(value);
    }
}

fn encode_len(encoder: &mut Encoder, length: usize) {
    encoder.u32(u32::try_from(length).expect("canonical sequence length must fit u32"));
}

fn decode_len(decoder: &mut Decoder<'_>) -> Result<usize, String> {
    let count = usize::try_from(decoder.u32().map_err(display)?)
        .map_err(|_| "profile count cannot fit this platform".to_owned())?;
    if count > MAX_PROFILE_OBLIGATIONS {
        return Err("application proof has too many profile obligations".to_owned());
    }
    Ok(count)
}

fn decode_profile(decoder: &mut Decoder<'_>) -> Result<NodeProfile, String> {
    Ok(NodeProfile {
        splitter2: decoder.u32().map_err(display)?,
        splitter3: decoder.u32().map_err(display)?,
        merger2: decoder.u32().map_err(display)?,
        merger3: decoder.u32().map_err(display)?,
    })
}

fn require_bytes(decoder: &mut Decoder<'_>, expected: &[u8]) -> Result<(), String> {
    if decoder.bytes().map_err(display)? == expected {
        Ok(())
    } else {
        Err("persistent application record magic does not match".to_owned())
    }
}

fn require_version(decoder: &mut Decoder<'_>, expected: u16) -> Result<(), String> {
    let actual = decoder.u16().map_err(display)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "unsupported application record version {actual}, expected {expected}"
        ))
    }
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use solver_api::{ConsumerPortRef, NodeId, NodeType, PhysicalNode, ProducerPortRef};
    use solver_core::{
        component_application::ComponentApplicationRequest,
        component_optimizer::{ComponentOptimizationOutcome, optimize_component_application},
        components::CanonicalComponentWitness,
    };

    use super::*;
    use crate::{ComponentDatabaseConfig, DatabaseMode};

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn proven_splitter() -> ProvenApplicationComponent {
        let incumbent = Component::import_canonical_witness(&CanonicalComponentWitness {
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
        .unwrap();
        let request = ComponentApplicationRequest::new(
            incumbent.boundary(),
            &[rational("2")],
            &[rational("1"), rational("1")],
            &rational("2"),
        )
        .unwrap();
        match optimize_component_application(request.key(), &incumbent, &AtomicBool::new(false))
            .unwrap()
        {
            ComponentOptimizationOutcome::Complete(proven) => *proven,
            ComponentOptimizationOutcome::Incomplete { .. } => panic!("unexpected cancellation"),
        }
    }

    fn memory() -> ComponentDatabase {
        ComponentDatabase::open(&ComponentDatabaseConfig {
            mode: DatabaseMode::InMemory,
            ..ComponentDatabaseConfig::default()
        })
        .unwrap()
    }

    #[test]
    fn proven_application_round_trips_only_as_a_feasible_incumbent() {
        let database = memory();
        let proven = proven_splitter();
        database.store_proven_application(&proven).unwrap().unwrap();
        let loaded = database
            .lookup_application_candidate(&proven.proof.key)
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded.component.export_record(),
            proven.component.export_record()
        );
        assert_eq!(loaded.application, proven.application);
        assert_eq!(loaded.stored_proof_summary, proven.proof);
        let through_port = ComponentRepository::load_complete(&database, &proven.proof.key)
            .unwrap()
            .unwrap();
        assert_eq!(
            through_port.component.export_record(),
            proven.component.export_record()
        );

        let changed_capacity = ComponentApplicationRequest::new(
            proven.component.boundary(),
            &[rational("2")],
            &[rational("1"), rational("1")],
            &rational("3"),
        )
        .unwrap();
        assert_eq!(
            database
                .lookup_application_candidate(changed_capacity.key())
                .unwrap(),
            None,
        );
    }

    #[test]
    fn repository_rejects_a_value_stored_under_a_different_concrete_key() {
        let database = memory();
        let proven = proven_splitter();
        let different = ComponentApplicationRequest::new(
            proven.component.boundary(),
            &[rational("4")],
            &[rational("2"), rational("2")],
            &rational("5"),
        )
        .unwrap();
        assert!(matches!(
            ComponentRepository::store_complete(&database, different.key(), &proven),
            Err(DatabaseError::InvalidApplicationProof)
        ));
    }

    #[test]
    fn incomplete_profile_manifest_cannot_be_stored_as_proven() {
        let database = memory();
        let mut proven = proven_splitter();
        proven.proof.exhausted_profiles.pop();
        assert!(matches!(
            database.store_proven_application(&proven),
            Err(DatabaseError::InvalidApplicationProof),
        ));
        assert_eq!(database.status().unwrap().application_records, 0);
    }
}
