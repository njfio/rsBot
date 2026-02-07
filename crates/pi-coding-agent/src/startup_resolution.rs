use super::*;

pub(crate) fn resolve_system_prompt(cli: &Cli) -> Result<String> {
    let Some(path) = cli.system_prompt_file.as_ref() else {
        return Ok(cli.system_prompt.clone());
    };

    let system_prompt = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read system prompt file {}", path.display()))?;

    ensure_non_empty_text(
        system_prompt,
        format!("system prompt file {}", path.display()),
    )
}

pub(crate) fn ensure_non_empty_text(text: String, source: String) -> Result<String> {
    if text.trim().is_empty() {
        bail!("{source} is empty");
    }
    Ok(text)
}

pub(crate) fn resolve_skill_trust_roots(cli: &Cli) -> Result<Vec<TrustedKey>> {
    let has_store_mutation = !cli.skill_trust_add.is_empty()
        || !cli.skill_trust_revoke.is_empty()
        || !cli.skill_trust_rotate.is_empty();
    if has_store_mutation && cli.skill_trust_root_file.is_none() {
        bail!("--skill-trust-root-file is required when using trust lifecycle flags");
    }

    let mut roots = Vec::new();
    for raw in &cli.skill_trust_root {
        roots.push(parse_trusted_root_spec(raw)?);
    }

    if let Some(path) = &cli.skill_trust_root_file {
        let mut records = load_trust_root_records(path)?;
        if has_store_mutation {
            let report = apply_trust_root_mutations(&mut records, cli)?;
            save_trust_root_records(path, &records)?;
            println!(
                "skill trust store update: added={} updated={} revoked={} rotated={}",
                report.added, report.updated, report.revoked, report.rotated
            );
        }

        let now_unix = current_unix_timestamp();
        for item in records {
            if item.revoked || is_expired_unix(item.expires_unix, now_unix) {
                continue;
            }
            roots.push(TrustedKey {
                id: item.id,
                public_key: item.public_key,
            });
        }
    }

    Ok(roots)
}
