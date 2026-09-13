//! Browser execution with independent compute heaps and a Rust proof coordinator.
//! The page owns termination and durable collection writes. No JS callback needs Sync.
mod coordinator;
mod leaf;
mod protocol;
#[cfg(test)]
mod tests;

pub use coordinator::BrowserCoordinator;
pub use leaf::BrowserLeaf;
use synthetizer_app::jobs::SolveRequest;

const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
#[must_use]
pub fn browser_solver_version() -> u32 {
    2
}

fn request(source: &str, id: &str) -> Result<SolveRequest, String> {
    if id.is_empty() || id.len() > 256 {
        return Err("invalid browser job identifier".into());
    }
    if source.len() > MAX_REQUEST_BYTES {
        return Err("Browser solve request exceeds 256 KiB.".into());
    }
    let request: SolveRequest = serde_json::from_str(source).map_err(|error| error.to_string())?;
    if request
        .problem
        .inputs
        .iter()
        .chain(&request.problem.outputs)
        .any(|endpoint| endpoint.id.len() > 256)
    {
        return Err("Endpoint identifier exceeds 256 bytes.".into());
    }
    Ok(request)
}
