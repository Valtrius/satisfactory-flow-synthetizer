//! Produce native golden responses for the same inputs executed by browser tests.
use std::io::{self, Read};

use serde_json::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let mut fixtures: Vec<Value> = serde_json::from_str(&input)?;
    for fixture in &mut fixtures {
        let payload = fixture["payload"]
            .as_str()
            .ok_or("missing fixture payload")?;
        let output = match fixture["operation"].as_str() {
            Some("verify") => solver_web::verify_witness_json(payload),
            Some("reconstruct") => solver_web::reconstruct_topology_json(payload),
            Some("share") => solver_web::verify_share_json(payload),
            Some("presentation-share") => solver_web::share_from_presentation_json(payload),
            _ => return Err("unknown fixture operation".into()),
        };
        fixture["native"] = serde_json::from_str(&output)?;
        if fixture["native"]["kind"] != fixture["expectedKind"] {
            return Err(format!(
                "unexpected native result for {}: {}",
                fixture["name"], fixture["native"]
            )
            .into());
        }
    }
    println!("{}", serde_json::to_string(&fixtures)?);
    Ok(())
}
