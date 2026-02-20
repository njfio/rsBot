use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;
use tau_memory::memory_contract::{MemoryEntry, MemoryScope};
use tau_memory::runtime::{
    FileMemoryStore, MemoryRelationInput, MemoryScopeFilter, MemorySearchMatch,
    MemorySearchOptions, MemoryType, RuntimeMemoryRecord,
};

use super::{
    authorize_and_enforce_gateway_limits, enforce_policy_gate, parse_gateway_json_body,
    record_cortex_memory_entry_delete_event, record_cortex_memory_entry_write_event,
    record_cortex_memory_write_event, sanitize_session_key, GatewayMemoryEntryDeleteRequest,
    GatewayMemoryEntryUpsertRequest, GatewayMemoryGraphEdge, GatewayMemoryGraphFilterSummary,
    GatewayMemoryGraphNode, GatewayMemoryGraphQuery, GatewayMemoryGraphResponse,
    GatewayMemoryReadQuery, GatewayMemoryUpdateRequest, GatewayOpenResponsesServerState,
    OpenResponsesApiError, DEFAULT_SESSION_KEY, MEMORY_WRITE_POLICY_GATE,
};

pub(super) async fn handle_gateway_memory_read(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    Query(query): Query<GatewayMemoryReadQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let session_key = sanitize_session_key(session_key.as_str());

    let search_query = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(search_query) = search_query {
        let memory_type_filter = match parse_gateway_memory_type(query.memory_type.as_deref()) {
            Ok(memory_type) => memory_type,
            Err(error) => return error.into_response(),
        };
        let options = MemorySearchOptions {
            limit: query.limit.unwrap_or(25).clamp(1, 200),
            scope: MemoryScopeFilter {
                workspace_id: normalize_optional_text(query.workspace_id),
                channel_id: normalize_optional_text(query.channel_id),
                actor_id: normalize_optional_text(query.actor_id),
            },
            ..MemorySearchOptions::default()
        };

        let store = gateway_memory_store(&state.config.state_dir, &session_key);
        let search_result = match store.search(search_query.as_str(), &options) {
            Ok(result) => result,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to search memory entries for session '{session_key}': {error}"
                ))
                .into_response();
            }
        };

        let mut matches = search_result
            .matches
            .iter()
            .filter(|entry| {
                memory_type_filter
                    .map(|expected| entry.memory_type == expected)
                    .unwrap_or(true)
            })
            .map(memory_search_match_json)
            .collect::<Vec<_>>();
        matches.truncate(options.limit);

        state.record_ui_telemetry_event("memory", "search", "memory_search_requested");
        return (
            StatusCode::OK,
            Json(json!({
                "mode": "search",
                "session_key": session_key,
                "query": search_query,
                "limit": options.limit,
                "memory_type_filter": memory_type_filter.map(|kind| kind.as_str()),
                "scope_filter": {
                    "workspace_id": options.scope.workspace_id,
                    "channel_id": options.scope.channel_id,
                    "actor_id": options.scope.actor_id,
                },
                "scanned": search_result.scanned,
                "returned": matches.len(),
                "retrieval_backend": search_result.retrieval_backend,
                "retrieval_reason_code": search_result.retrieval_reason_code,
                "embedding_backend": search_result.embedding_backend,
                "embedding_reason_code": search_result.embedding_reason_code,
                "matches": matches,
                "storage_backend": store.storage_backend_label(),
                "storage_reason_code": store.storage_backend_reason_code(),
                "store_root": gateway_memory_store_root(&state.config.state_dir, &session_key).display().to_string(),
            })),
        )
            .into_response();
    }

    let path = gateway_memory_path(&state.config.state_dir, &session_key);
    let exists = path.exists();
    let content = if exists {
        match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to read memory '{}': {error}",
                    path.display()
                ))
                .into_response();
            }
        }
    } else {
        String::new()
    };

    state.record_ui_telemetry_event("memory", "read", "memory_read_requested");
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": path.display().to_string(),
            "exists": exists,
            "bytes": content.len(),
            "content": content,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_memory_write(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryUpdateRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event("memory", "write", "memory_write_policy_gate_blocked");
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let memory_path = gateway_memory_path(&state.config.state_dir, &session_key);
    if let Some(parent) = memory_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return OpenResponsesApiError::internal(format!(
                "failed to create memory directory '{}': {error}",
                parent.display()
            ))
            .into_response();
        }
    }
    let mut content = request.content;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    if let Err(error) = std::fs::write(&memory_path, content.as_bytes()) {
        return OpenResponsesApiError::internal(format!(
            "failed to write memory '{}': {error}",
            memory_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event("memory", "write", "memory_write_applied");
    record_cortex_memory_write_event(&state.config.state_dir, session_key.as_str(), content.len());
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": memory_path.display().to_string(),
            "bytes": content.len(),
            "updated_unix_ms": current_unix_timestamp_ms(),
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_memory_entry_read(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    match store.read_entry(entry_id.as_str(), None) {
        Ok(Some(record)) => {
            state.record_ui_telemetry_event("memory", "entry_read", "memory_entry_read_requested");
            (
                StatusCode::OK,
                Json(json!({
                    "session_key": session_key,
                    "entry": memory_record_json(&record),
                    "storage_backend": store.storage_backend_label(),
                    "storage_reason_code": store.storage_backend_reason_code(),
                })),
            )
                .into_response()
        }
        Ok(None) => OpenResponsesApiError::not_found(
            "memory_entry_not_found",
            format!(
                "memory entry '{}' was not found for session '{}'",
                entry_id, session_key
            ),
        )
        .into_response(),
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to read memory entry '{}' for session '{}': {error}",
            entry_id, session_key
        ))
        .into_response(),
    }
}

pub(super) async fn handle_gateway_memory_entry_write(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryEntryUpsertRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event(
            "memory",
            "entry_write",
            "memory_entry_write_policy_gate_blocked",
        );
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }
    let summary = request.summary.trim().to_string();
    if summary.is_empty() {
        return OpenResponsesApiError::bad_request("invalid_summary", "summary must be non-empty")
            .into_response();
    }
    let memory_type = match parse_gateway_memory_type(request.memory_type.as_deref()) {
        Ok(memory_type) => memory_type,
        Err(error) => return error.into_response(),
    };

    let scope = MemoryScope {
        workspace_id: normalize_optional_text(request.workspace_id)
            .unwrap_or_else(|| session_key.clone()),
        channel_id: normalize_optional_text(request.channel_id)
            .unwrap_or_else(|| "gateway".to_string()),
        actor_id: normalize_optional_text(request.actor_id)
            .unwrap_or_else(|| "operator".to_string()),
    };
    let entry = MemoryEntry {
        memory_id: entry_id.clone(),
        summary,
        tags: request.tags,
        facts: request.facts,
        source_event_key: request.source_event_key,
        recency_weight_bps: 0,
        confidence_bps: 1000,
    };
    let relation_inputs = request
        .relations
        .into_iter()
        .map(|relation| MemoryRelationInput {
            target_id: relation.target_id,
            relation_type: relation.relation_type,
            weight: relation.weight,
        })
        .collect::<Vec<_>>();

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    let write_result = match store.write_entry_with_metadata_and_relations(
        &scope,
        entry,
        memory_type,
        request.importance,
        relation_inputs.as_slice(),
    ) {
        Ok(result) => result,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to write memory entry '{}' for session '{}': {error}",
                entry_id, session_key
            ))
            .into_response();
        }
    };

    state.record_ui_telemetry_event("memory", "entry_write", "memory_entry_write_applied");
    record_cortex_memory_entry_write_event(
        &state.config.state_dir,
        session_key.as_str(),
        entry_id.as_str(),
        write_result.created,
    );
    (
        if write_result.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "session_key": session_key,
            "created": write_result.created,
            "entry": memory_record_json(&write_result.record),
            "storage_backend": store.storage_backend_label(),
            "storage_reason_code": store.storage_backend_reason_code(),
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_memory_entry_delete(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryEntryDeleteRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event(
            "memory",
            "entry_delete",
            "memory_entry_delete_policy_gate_blocked",
        );
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    match store.soft_delete_entry(entry_id.as_str(), None) {
        Ok(Some(record)) => {
            state.record_ui_telemetry_event(
                "memory",
                "entry_delete",
                "memory_entry_delete_applied",
            );
            record_cortex_memory_entry_delete_event(
                &state.config.state_dir,
                session_key.as_str(),
                entry_id.as_str(),
                true,
            );
            (
                StatusCode::OK,
                Json(json!({
                    "session_key": session_key,
                    "deleted": true,
                    "entry": memory_record_json(&record),
                    "storage_backend": store.storage_backend_label(),
                    "storage_reason_code": store.storage_backend_reason_code(),
                })),
            )
                .into_response()
        }
        Ok(None) => OpenResponsesApiError::not_found(
            "memory_entry_not_found",
            format!(
                "memory entry '{}' was not found for session '{}'",
                entry_id, session_key
            ),
        )
        .into_response(),
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to delete memory entry '{}' for session '{}': {error}",
            entry_id, session_key
        ))
        .into_response(),
    }
}

pub(super) async fn handle_gateway_memory_graph(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    Query(query): Query<GatewayMemoryGraphQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    match build_gateway_memory_graph_response(&state, session_key.as_str(), &query) {
        Ok(payload) => {
            state.record_ui_telemetry_event("memory", "graph", "memory_graph_requested");
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(error) => error.into_response(),
    }
}

pub(super) async fn handle_api_memories_graph(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    Query(query): Query<GatewayMemoryGraphQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let requested_session = query.session_key.as_deref().unwrap_or(DEFAULT_SESSION_KEY);
    match build_gateway_memory_graph_response(&state, requested_session, &query) {
        Ok(payload) => {
            state.record_ui_telemetry_event("memory", "graph", "memory_graph_requested");
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(error) => error.into_response(),
    }
}

fn build_gateway_memory_graph_response(
    state: &GatewayOpenResponsesServerState,
    session_key_raw: &str,
    query: &GatewayMemoryGraphQuery,
) -> Result<GatewayMemoryGraphResponse, OpenResponsesApiError> {
    let session_key = sanitize_session_key(session_key_raw);
    let memory_path = gateway_memory_path(&state.config.state_dir, session_key.as_str());
    let exists = memory_path.exists();
    let content = if exists {
        std::fs::read_to_string(&memory_path).map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to read memory '{}': {error}",
                memory_path.display()
            ))
        })?
    } else {
        String::new()
    };

    let max_nodes = query.max_nodes.unwrap_or(24).clamp(1, 256);
    let min_edge_weight = query.min_edge_weight.unwrap_or(1.0).max(0.0);
    let relation_types = normalize_memory_graph_relation_types(query.relation_types.as_deref());
    let nodes = build_memory_graph_nodes(&content, max_nodes);
    let edges = build_memory_graph_edges(&nodes, &relation_types, min_edge_weight);

    Ok(GatewayMemoryGraphResponse {
        session_key,
        path: memory_path.display().to_string(),
        exists,
        bytes: content.len(),
        node_count: nodes.len(),
        edge_count: edges.len(),
        nodes,
        edges,
        filters: GatewayMemoryGraphFilterSummary {
            max_nodes,
            min_edge_weight,
            relation_types,
        },
    })
}

fn normalize_memory_graph_relation_types(raw: Option<&str>) -> Vec<String> {
    let mut relation_types = raw
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|value| value == "contains" || value == "keyword_overlap")
        .collect::<BTreeSet<_>>();
    if relation_types.is_empty() {
        relation_types.insert("contains".to_string());
        relation_types.insert("keyword_overlap".to_string());
    }
    relation_types.into_iter().collect()
}

fn build_memory_graph_nodes(content: &str, max_nodes: usize) -> Vec<GatewayMemoryGraphNode> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(max_nodes)
        .enumerate()
        .map(|(index, line)| {
            let term_count = memory_graph_terms(line).len().max(1);
            GatewayMemoryGraphNode {
                id: format!("line:{}", index + 1),
                label: line.to_string(),
                category: "memory_line".to_string(),
                weight: term_count as f64,
                size: 12.0 + (term_count.min(8) as f64 * 2.0),
            }
        })
        .collect()
}

fn build_memory_graph_edges(
    nodes: &[GatewayMemoryGraphNode],
    relation_types: &[String],
    min_edge_weight: f64,
) -> Vec<GatewayMemoryGraphEdge> {
    let relation_filter = relation_types
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let normalized_labels = nodes
        .iter()
        .map(|node| node.label.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let terms = nodes
        .iter()
        .map(|node| memory_graph_terms(&node.label))
        .collect::<Vec<_>>();
    let mut edges = Vec::new();

    for left_index in 0..nodes.len() {
        for right_index in (left_index + 1)..nodes.len() {
            let left = &nodes[left_index];
            let right = &nodes[right_index];
            let left_label = normalized_labels[left_index].as_str();
            let right_label = normalized_labels[right_index].as_str();

            if relation_filter.contains("contains")
                && left_label != right_label
                && !left_label.is_empty()
                && !right_label.is_empty()
            {
                let relation_direction = if left_label.contains(right_label) {
                    Some((left.id.as_str(), right.id.as_str()))
                } else if right_label.contains(left_label) {
                    Some((right.id.as_str(), left.id.as_str()))
                } else {
                    None
                };
                if let Some((source, target)) = relation_direction {
                    let weight = 1.0;
                    if weight >= min_edge_weight {
                        edges.push(GatewayMemoryGraphEdge {
                            id: format!("edge:contains:{source}:{target}"),
                            source: source.to_string(),
                            target: target.to_string(),
                            relation_type: "contains".to_string(),
                            weight,
                        });
                    }
                }
            }

            if relation_filter.contains("keyword_overlap") {
                let overlap = terms[left_index].intersection(&terms[right_index]).count();
                if overlap > 0 {
                    let weight = overlap as f64;
                    if weight >= min_edge_weight {
                        edges.push(GatewayMemoryGraphEdge {
                            id: format!("edge:keyword_overlap:{}:{}", left.id, right.id),
                            source: left.id.clone(),
                            target: right.id.clone(),
                            relation_type: "keyword_overlap".to_string(),
                            weight,
                        });
                    }
                }
            }
        }
    }

    edges.sort_by(|left, right| {
        (
            left.relation_type.as_str(),
            left.source.as_str(),
            left.target.as_str(),
            left.id.as_str(),
        )
            .cmp(&(
                right.relation_type.as_str(),
                right.source.as_str(),
                right.target.as_str(),
                right.id.as_str(),
            ))
    });
    edges
}

fn memory_graph_terms(value: &str) -> BTreeSet<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_lowercase)
        .filter(|segment| segment.len() >= 3)
        .collect()
}

fn gateway_memory_path(state_dir: &Path, session_key: &str) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("memory")
        .join(format!("{session_key}.md"))
}

fn gateway_memory_store_root(state_dir: &Path, session_key: &str) -> PathBuf {
    gateway_memory_stores_root(state_dir).join(session_key)
}

pub(super) fn gateway_memory_store(state_dir: &Path, session_key: &str) -> FileMemoryStore {
    FileMemoryStore::new(gateway_memory_store_root(state_dir, session_key))
}

pub(super) fn gateway_memory_stores_root(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("memory-store")
}

fn normalize_optional_text(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_gateway_memory_type(
    raw: Option<&str>,
) -> Result<Option<MemoryType>, OpenResponsesApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    MemoryType::parse(raw).map(Some).ok_or_else(|| {
        OpenResponsesApiError::bad_request(
            "invalid_memory_type",
            "memory_type must be one of: identity, goal, decision, todo, preference, fact, event, observation",
        )
    })
}

fn memory_record_json(record: &RuntimeMemoryRecord) -> Value {
    json!({
        "memory_id": record.entry.memory_id.as_str(),
        "summary": record.entry.summary.as_str(),
        "tags": &record.entry.tags,
        "facts": &record.entry.facts,
        "source_event_key": record.entry.source_event_key.as_str(),
        "scope": {
            "workspace_id": record.scope.workspace_id.as_str(),
            "channel_id": record.scope.channel_id.as_str(),
            "actor_id": record.scope.actor_id.as_str(),
        },
        "memory_type": record.memory_type.as_str(),
        "importance": record.importance,
        "relations": &record.relations,
        "embedding_source": record.embedding_source.as_str(),
        "embedding_model": &record.embedding_model,
        "embedding_vector_dim": record.embedding_vector.len(),
        "embedding_reason_code": record.embedding_reason_code.as_str(),
        "updated_unix_ms": record.updated_unix_ms,
        "last_accessed_at_unix_ms": record.last_accessed_at_unix_ms,
        "access_count": record.access_count,
        "forgotten": record.forgotten,
    })
}

fn memory_search_match_json(entry: &MemorySearchMatch) -> Value {
    json!({
        "memory_id": entry.memory_id.as_str(),
        "score": entry.score,
        "vector_score": entry.vector_score,
        "lexical_score": entry.lexical_score,
        "fused_score": entry.fused_score,
        "graph_score": entry.graph_score,
        "scope": {
            "workspace_id": entry.scope.workspace_id.as_str(),
            "channel_id": entry.scope.channel_id.as_str(),
            "actor_id": entry.scope.actor_id.as_str(),
        },
        "summary": entry.summary.as_str(),
        "memory_type": entry.memory_type.as_str(),
        "importance": entry.importance,
        "tags": &entry.tags,
        "facts": &entry.facts,
        "source_event_key": entry.source_event_key.as_str(),
        "embedding_source": entry.embedding_source.as_str(),
        "embedding_model": &entry.embedding_model,
        "relations": &entry.relations,
    })
}
