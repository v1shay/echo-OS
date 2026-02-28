use anyhow::Result;

pub async fn execute_safe(_cmd: &str, _args: &[String]) -> Result<()> {
    // TODO: implement allowlist/denylist, sandboxing, and safe process spawning.
    anyhow::bail!("executor not implemented")
}
