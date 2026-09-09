//! Optional per-root evidence; disabled timing runs do not collect phase clocks.
use std::time::{Duration, Instant};

pub(crate) struct RootStats {
    pub started: Instant,
    pub check: Duration,
    pub validation: Duration,
    pub identity: Duration,
    pub models: u64,
    pub duplicates: u64,
    pub valid: u64,
}

impl Default for RootStats {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            check: Duration::ZERO,
            validation: Duration::ZERO,
            identity: Duration::ZERO,
            models: 0,
            duplicates: 0,
            valid: 0,
        }
    }
}

impl RootStats {
    pub fn record(
        &self,
        root: usize,
        nodes: u32,
        links: u32,
        root_info: &crate::Root,
        state: &str,
        origin: Instant,
    ) -> String {
        let profile = root_info.profile;
        let source = root_info
            .source
            .map_or_else(|| "null".to_owned(), |v| v.to_string());
        format!(
            "{{\"root\":{root},\"profile\":{profile},\"nodes\":{nodes},\"links\":{links},\"source\":{source},\"state\":\"{state}\",\"start_s\":{},\"wall_s\":{},\"check_s\":{},\"validation_s\":{},\"identity_s\":{},\"models\":{},\"duplicates\":{},\"valid\":{}}}",
            self.started.duration_since(origin).as_secs_f64(),
            self.started.elapsed().as_secs_f64(),
            self.check.as_secs_f64(),
            self.validation.as_secs_f64(),
            self.identity.as_secs_f64(),
            self.models,
            self.duplicates,
            self.valid
        )
    }
}
