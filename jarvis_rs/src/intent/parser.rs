use anyhow::Result;

use crate::intent::types::Intent;

pub async fn parse_intent(_input: &str) -> Result<Intent> {
    // TODO: implement intent parsing.
    anyhow::bail!("intent parsing not implemented")
}
