use anyhow::Result;

use crate::safety::risk::RiskLevel;

pub fn assess_risk(_input: &str) -> Result<RiskLevel> {
    // TODO: implement validation + risk classification.
    anyhow::bail!("risk assessment not implemented")
}
