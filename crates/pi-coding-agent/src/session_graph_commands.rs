use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionGraphFormat {
    Mermaid,
    Dot,
}

impl SessionGraphFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid",
            Self::Dot => "dot",
        }
    }
}

pub(crate) fn resolve_session_graph_format(path: &Path) -> SessionGraphFormat {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("dot") {
        SessionGraphFormat::Dot
    } else {
        SessionGraphFormat::Mermaid
    }
}

pub(crate) fn escape_graph_label(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn session_graph_node_label(entry: &crate::session::SessionEntry) -> String {
    format!(
        "{}: {} | {}",
        entry.id,
        session_message_role(&entry.message),
        session_message_preview(&entry.message)
    )
}

pub(crate) fn render_session_graph_mermaid(entries: &[crate::session::SessionEntry]) -> String {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.id);

    let mut lines = vec!["graph TD".to_string()];
    if ordered.is_empty() {
        lines.push("  empty[\"(empty session)\"]".to_string());
        return lines.join("\n");
    }

    for entry in &ordered {
        lines.push(format!(
            "  n{}[\"{}\"]",
            entry.id,
            escape_graph_label(&session_graph_node_label(entry))
        ));
    }
    for entry in &ordered {
        if let Some(parent_id) = entry.parent_id {
            lines.push(format!("  n{} --> n{}", parent_id, entry.id));
        }
    }
    lines.join("\n")
}

pub(crate) fn render_session_graph_dot(entries: &[crate::session::SessionEntry]) -> String {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.id);

    let mut lines = vec!["digraph session {".to_string(), "  rankdir=LR;".to_string()];
    if ordered.is_empty() {
        lines.push("  empty [label=\"(empty session)\"];".to_string());
    } else {
        for entry in &ordered {
            lines.push(format!(
                "  n{} [label=\"{}\"];",
                entry.id,
                escape_graph_label(&session_graph_node_label(entry))
            ));
        }
        for entry in &ordered {
            if let Some(parent_id) = entry.parent_id {
                lines.push(format!("  n{} -> n{};", parent_id, entry.id));
            }
        }
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn render_session_graph(
    format: SessionGraphFormat,
    entries: &[crate::session::SessionEntry],
) -> String {
    match format {
        SessionGraphFormat::Mermaid => render_session_graph_mermaid(entries),
        SessionGraphFormat::Dot => render_session_graph_dot(entries),
    }
}

pub(crate) fn execute_session_graph_export_command(
    runtime: &SessionRuntime,
    command_args: &str,
) -> String {
    let destination = PathBuf::from(command_args.trim());
    let format = resolve_session_graph_format(&destination);
    let graph = render_session_graph(format, runtime.store.entries());
    let nodes = runtime.store.entries().len();
    let edges = runtime
        .store
        .entries()
        .iter()
        .filter(|entry| entry.parent_id.is_some())
        .count();

    match write_text_atomic(&destination, &graph) {
        Ok(()) => format!(
            "session graph export: path={} format={} nodes={} edges={}",
            destination.display(),
            format.as_str(),
            nodes,
            edges
        ),
        Err(error) => format!(
            "session graph export error: path={} error={error}",
            destination.display()
        ),
    }
}
