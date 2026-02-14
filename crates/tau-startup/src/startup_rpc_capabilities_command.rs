use anyhow::Result;
use tau_cli::Cli;

pub fn execute_rpc_capabilities_command(cli: &Cli) -> Result<()> {
    if !cli.rpc_capabilities {
        return Ok(());
    }

    let payload = tau_runtime::render_rpc_capabilities_payload_pretty()?;
    println!("{payload}");
    Ok(())
}
