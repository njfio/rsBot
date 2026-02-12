use anyhow::Result;

use crate::Cli;

use tau_runtime::render_rpc_capabilities_payload_pretty;
#[cfg(test)]
pub(crate) use tau_runtime::rpc_capabilities_payload;

pub(crate) fn execute_rpc_capabilities_command(cli: &Cli) -> Result<()> {
    if !cli.rpc_capabilities {
        return Ok(());
    }

    let payload = render_rpc_capabilities_payload_pretty()?;
    println!("{payload}");
    Ok(())
}
