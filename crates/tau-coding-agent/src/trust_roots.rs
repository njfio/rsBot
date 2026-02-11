pub use tau_access::trust_roots::*;

use anyhow::Result;

use crate::Cli;

pub(crate) fn apply_trust_root_mutations(
    records: &mut Vec<TrustedRootRecord>,
    cli: &Cli,
) -> Result<TrustMutationReport> {
    apply_trust_root_mutation_specs(
        records,
        &cli.skill_trust_add,
        &cli.skill_trust_revoke,
        &cli.skill_trust_rotate,
    )
}
