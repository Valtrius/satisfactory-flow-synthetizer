use crate::protocol::{LeafWork, encode};
use serde::Serialize;
use solver_api::BestKnownSolution;
use solver_core::{
    leaf::{Completion, LeafContext, LeafDriver},
    profile::profile_link_accountings,
};

#[derive(Serialize)]
struct Action {
    commands: String,
    witness: Option<BestKnownSolution>,
    completion: Option<&'static str>,
}

/// One worker-local leaf, independent of every other Rust and cvc5 heap.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct BrowserLeaf {
    driver: LeafDriver,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl BrowserLeaf {
    /// Construct only from coordinator-generated work, never imported proof claims.
    /// # Errors
    /// Rejects malformed work and inconsistent physical accounting.
    #[cfg_attr(
        target_arch = "wasm32",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new(source: &str) -> Result<Self, String> {
        if source.len() > crate::MAX_EVENT_BYTES {
            return Err("browser leaf work exceeds 16 MiB".into());
        }
        let work: LeafWork = serde_json::from_str(source).map_err(|error| error.to_string())?;
        let spec = work.spec();
        let context = LeafContext::prepare(work.problem).map_err(|error| error.to_string())?;
        let accountings = profile_link_accountings(
            spec.profile.profile,
            u32::try_from(context.problem.inputs.len()).map_err(|error| error.to_string())?,
            u32::try_from(context.problem.outputs.len()).map_err(|error| error.to_string())?,
            &context.normalized.surplus,
            &context.normalized.max_link_rate,
        )
        .map_err(|error| error.to_string())?;
        if !accountings.contains(&spec.profile.accounting) {
            return Err("invalid browser leaf accounting".into());
        }
        Ok(Self {
            driver: LeafDriver::new(&context, spec),
        })
    }

    /// Run one exact leaf transition. Publish the witness before the next check.
    /// # Errors
    /// Returns poisoned protocol, reconstruction and validation errors.
    #[allow(clippy::needless_pass_by_value)]
    pub fn advance(&mut self, reply: Option<String>) -> Result<String, String> {
        let action = self
            .driver
            .advance(reply.as_deref(), false, &mut ())
            .map_err(|error| error.to_string())?;
        encode(&Action {
            commands: action.commands,
            witness: action.witness,
            completion: action.completion.map(|value| match value {
                Completion::Exhausted => "exhausted",
                Completion::Optimum => "optimum",
            }),
        })
    }
}
