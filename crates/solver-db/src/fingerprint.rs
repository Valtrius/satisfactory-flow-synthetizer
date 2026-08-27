use sha2::{Digest, Sha256};

/// Human-auditable description hashed into every persistent proof record.
///
/// Changing any correctness-visible semantics requires changing this string.
pub const SEMANTIC_SCHEMA_DESCRIPTION: &str = concat!(
    "satisfactory-exact-component-db/v1\n",
    "rational=canonical-arbitrary-precision\n",
    "nodes=splitter2,splitter3,merger2,merger3-effective-arity\n",
    "splitter=equal-positive-outputs-and-conservation\n",
    "merger=positive-input-sum\n",
    "physical-link=exact-strict-0<f<=B\n",
    "objective=lex(nodes,non-discard-links);discard-links-excluded-from-L\n",
    "discard=anonymous-external-sinks-exact-surplus-sum\n",
    "frozen-subsystem=immutable-internal-topology-declared-boundary-v2\n",
    "component=exact-R,T,K,strict-domain,capacity-behavior\n",
    "canonical-witness=satisfactory-canonical-graph-v1-discard-tag2\n",
    "component-record=manual-binary-v1\n",
    "component-frontier=global-domain-cost-peak-dominance-proof-v1\n",
    "application-proof=concrete-common-scale-boundary-and-capacity-v1\n",
    "prewarm=bounded-cells-root-partitions-manifest-membership-v1\n",
    "validator=2\n",
);

/// Stable SHA-256 semantic namespace for compatible persistent records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticFingerprint([u8; 32]);

impl SemanticFingerprint {
    /// Computes the fingerprint for the currently compiled solver semantics.
    #[must_use]
    pub fn current() -> Self {
        Self(Sha256::digest(SEMANTIC_SCHEMA_DESCRIPTION.as_bytes()).into())
    }

    /// Constructs a fingerprint from persisted bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw digest.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Content-addresses a typed canonical payload inside one semantic namespace.
#[must_use]
pub fn content_id(fingerprint: SemanticFingerprint, record_tag: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(fingerprint.as_bytes());
    hasher.update(
        u64::try_from(record_tag.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    hasher.update(record_tag);
    hasher.update(
        u64::try_from(payload.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    hasher.update(payload);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_content_ids_are_stable_and_domain_separated() {
        let fingerprint = SemanticFingerprint::current();
        assert_eq!(fingerprint, SemanticFingerprint::current());
        assert_eq!(
            content_id(fingerprint, b"component", b"payload"),
            content_id(fingerprint, b"component", b"payload")
        );
        assert_ne!(
            content_id(fingerprint, b"component", b"payload"),
            content_id(fingerprint, b"application", b"payload")
        );
        assert_ne!(
            content_id(fingerprint, b"component", b"payload"),
            content_id(fingerprint, b"component", b"payload-2")
        );
    }
}
