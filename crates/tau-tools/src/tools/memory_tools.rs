//! Memory tool definitions and execution wiring.

use super::*;

const MEMORY_TYPE_ENUM_VALUES: &[&str] = &[
    "identity",
    "goal",
    "decision",
    "todo",
    "preference",
    "fact",
    "event",
    "observation",
];
const MEMORY_RELATION_TYPE_ENUM_VALUES: &[&str] = &[
    "relates_to",
    "depends_on",
    "supports",
    "blocks",
    "references",
];
const MEMORY_GRAPH_SIGNAL_WEIGHT: f32 = 0.25;

fn optional_memory_type(arguments: &Value) -> Result<Option<MemoryType>, String> {
    let Some(value) = optional_string(arguments, "memory_type")? else {
        return Ok(None);
    };
    MemoryType::parse(value.as_str()).map(Some).ok_or_else(|| {
        format!(
            "'memory_type' must be one of: {}",
            MEMORY_TYPE_ENUM_VALUES.join(", ")
        )
    })
}

fn optional_memory_relations(arguments: &Value) -> Result<Vec<MemoryRelationInput>, String> {
    let Some(raw_relations) = arguments.get("relates_to") else {
        return Ok(Vec::new());
    };
    let Some(relations) = raw_relations.as_array() else {
        return Err("'relates_to' must be an array of relation objects".to_string());
    };
    let mut parsed = Vec::with_capacity(relations.len());
    for (index, relation) in relations.iter().enumerate() {
        let Some(relation_object) = relation.as_object() else {
            return Err(format!("'relates_to[{index}]' must be an object"));
        };
        let Some(target_id) = relation_object
            .get("target_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(format!(
                "'relates_to[{index}].target_id' must be a non-empty string"
            ));
        };
        let relation_type = relation_object
            .get("relation_type")
            .map(|value| {
                value
                    .as_str()
                    .map(str::trim)
                    .ok_or_else(|| format!("'relates_to[{index}].relation_type' must be a string"))
                    .map(|value| value.to_string())
            })
            .transpose()?
            .filter(|value| !value.is_empty());
        let weight = relation_object
            .get("weight")
            .map(|value| {
                value
                    .as_f64()
                    .ok_or_else(|| format!("'relates_to[{index}].weight' must be a number"))
                    .map(|value| value as f32)
            })
            .transpose()?;
        parsed.push(MemoryRelationInput {
            target_id: target_id.to_string(),
            relation_type,
            weight,
        });
    }
    Ok(parsed)
}

/// Public struct `MemoryWriteTool` used across Tau components.
pub struct MemoryWriteTool {
    policy: Arc<ToolPolicy>,
}

impl MemoryWriteTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for MemoryWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_write".to_string(),
            description:
                "Write or update a scoped semantic memory entry in the runtime memory store"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Optional stable memory id. A deterministic id is generated when omitted."
                    },
                    "summary": {
                        "type": "string",
                        "description": format!(
                            "Memory summary text (max {} characters)",
                            self.policy.memory_write_max_summary_chars
                        )
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": format!(
                            "Optional tags (max {} items, {} chars each)",
                            self.policy.memory_write_max_tags,
                            self.policy.memory_write_max_tag_chars
                        )
                    },
                    "facts": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": format!(
                            "Optional facts (max {} items, {} chars each)",
                            self.policy.memory_write_max_facts,
                            self.policy.memory_write_max_fact_chars
                        )
                    },
                    "workspace_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "actor_id": { "type": "string" },
                    "source_event_key": { "type": "string" },
                    "recency_weight_bps": {
                        "type": "integer",
                        "description": "Optional recency weight in basis points (0..=10000)"
                    },
                    "confidence_bps": {
                        "type": "integer",
                        "description": "Optional confidence score in basis points (0..=10000)"
                    },
                    "memory_type": {
                        "type": "string",
                        "enum": MEMORY_TYPE_ENUM_VALUES,
                        "description": "Optional typed-memory classification"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Optional importance override in range 0.0..=1.0"
                    },
                    "relates_to": {
                        "type": "array",
                        "description": "Optional outbound relation edges from this memory",
                        "items": {
                            "type": "object",
                            "properties": {
                                "target_id": { "type": "string", "description": "Existing memory id this entry points to" },
                                "relation_type": {
                                    "type": "string",
                                    "enum": MEMORY_RELATION_TYPE_ENUM_VALUES,
                                    "description": "Optional relation type (defaults to relates_to)"
                                },
                                "weight": {
                                    "type": "number",
                                    "description": "Optional relation weight in range 0.0..=1.0 (defaults to 1.0)"
                                }
                            },
                            "required": ["target_id"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["summary"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let summary = match required_string(&arguments, "summary") {
            Ok(summary) => summary.trim().to_string(),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        if summary.is_empty() {
            return ToolExecutionResult::error(json!({
                "tool": "memory_write",
                "reason_code": "memory_empty_summary",
                "error": "summary must not be empty",
            }));
        }
        if summary.chars().count() > self.policy.memory_write_max_summary_chars {
            return ToolExecutionResult::error(json!({
                "tool": "memory_write",
                "reason_code": "memory_summary_too_large",
                "max_summary_chars": self.policy.memory_write_max_summary_chars,
                "error": format!(
                    "summary exceeds max length of {} characters",
                    self.policy.memory_write_max_summary_chars
                ),
            }));
        }

        let memory_id = arguments
            .get("memory_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(generate_memory_id);

        let tags = match optional_string_array(
            &arguments,
            "tags",
            self.policy.memory_write_max_tags,
            self.policy.memory_write_max_tag_chars,
        ) {
            Ok(tags) => tags,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let facts = match optional_string_array(
            &arguments,
            "facts",
            self.policy.memory_write_max_facts,
            self.policy.memory_write_max_fact_chars,
        ) {
            Ok(facts) => facts,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let recency_weight_bps = match optional_basis_points(&arguments, "recency_weight_bps") {
            Ok(value) => value.unwrap_or(0),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let confidence_bps = match optional_basis_points(&arguments, "confidence_bps") {
            Ok(value) => value.unwrap_or(0),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let memory_type = match optional_memory_type(&arguments) {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let importance = match optional_unit_interval_f32(&arguments, "importance") {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let relations = match optional_memory_relations(&arguments) {
            Ok(relations) => relations,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": "memory_invalid_relation",
                    "error": error,
                }))
            }
        };

        let scope = MemoryScope {
            workspace_id: arguments
                .get("workspace_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            channel_id: arguments
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            actor_id: arguments
                .get("actor_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        let source_event_key = arguments
            .get("source_event_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let entry = MemoryEntry {
            memory_id: memory_id.clone(),
            summary: summary.clone(),
            tags,
            facts,
            source_event_key,
            recency_weight_bps,
            confidence_bps,
        };
        let store = FileMemoryStore::new_with_embedding_provider_and_importance_profile(
            self.policy.memory_state_dir.clone(),
            self.policy.memory_embedding_provider_config(),
            Some(self.policy.memory_default_importance_profile.clone()),
        );
        let storage_path = store
            .storage_path()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| store.root_dir().join("memory.backend"));

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "memory_write",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "memory_id": memory_id.clone(),
                "scope": {
                    "workspace_id": scope.workspace_id.clone(),
                    "channel_id": scope.channel_id.clone(),
                    "actor_id": scope.actor_id.clone(),
                },
                "summary_chars": summary.chars().count(),
                "tags": entry.tags.clone(),
                "facts": entry.facts.clone(),
                "memory_type": memory_type.map(MemoryType::as_str),
                "importance": importance,
                "relates_to": relations
                    .iter()
                    .map(|relation| json!({
                        "target_id": relation.target_id,
                        "relation_type": relation.relation_type,
                        "weight": relation.weight,
                    }))
                    .collect::<Vec<_>>(),
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolWrite {
            path: storage_path.display().to_string(),
            content_bytes: summary.len(),
        }) {
            return approval_result;
        }

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "memory_write",
            json!({
                "scope": {
                    "workspace_id": scope.workspace_id.clone(),
                    "channel_id": scope.channel_id.clone(),
                    "actor_id": scope.actor_id.clone(),
                },
                "summary_chars": summary.chars().count(),
                "relation_edges": relations.len(),
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
            }),
        ) {
            return rate_limit_result;
        }

        match store.write_entry_with_metadata_and_relations(
            &scope,
            entry,
            memory_type,
            importance,
            &relations,
        ) {
            Ok(result) => ToolExecutionResult::ok(json!({
                "tool": "memory_write",
                "created": result.created,
                "memory_id": result.record.entry.memory_id,
                "scope": result.record.scope,
                "summary": result.record.entry.summary,
                "memory_type": result.record.memory_type.as_str(),
                "importance": result.record.importance,
                "tags": result.record.entry.tags,
                "facts": result.record.entry.facts,
                "source_event_key": result.record.entry.source_event_key,
                "recency_weight_bps": result.record.entry.recency_weight_bps,
                "confidence_bps": result.record.entry.confidence_bps,
                "updated_unix_ms": result.record.updated_unix_ms,
                "last_accessed_at_unix_ms": result.record.last_accessed_at_unix_ms,
                "access_count": result.record.access_count,
                "forgotten": result.record.forgotten,
                "embedding_source": result.record.embedding_source,
                "embedding_model": result.record.embedding_model,
                "embedding_reason_code": result.record.embedding_reason_code,
                "relations": result.record.relations,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Err(error) => {
                let error_message = error.to_string();
                let reason_code = if error_message.starts_with("memory_invalid_relation:") {
                    "memory_invalid_relation"
                } else {
                    "memory_backend_error"
                };
                ToolExecutionResult::error(json!({
                    "tool": "memory_write",
                    "reason_code": reason_code,
                    "store_root": store.root_dir().display().to_string(),
                    "storage_backend": store.storage_backend_label(),
                    "backend_reason_code": store.storage_backend_reason_code(),
                    "storage_path": store
                        .storage_path()
                        .map(|path| path.display().to_string()),
                    "error": error_message,
                }))
            }
        }
    }
}

/// Public struct `MemoryReadTool` used across Tau components.
pub struct MemoryReadTool {
    policy: Arc<ToolPolicy>,
}

impl MemoryReadTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for MemoryReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_read".to_string(),
            description: "Read a scoped semantic memory entry by id".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory id to load" },
                    "workspace_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "actor_id": { "type": "string" }
                },
                "required": ["memory_id"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let memory_id = match required_string(&arguments, "memory_id") {
            Ok(memory_id) => memory_id,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_read",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };

        let scope_filter = memory_scope_filter_from_arguments(&arguments);
        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "memory_read",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "memory_id": memory_id.clone(),
                "scope_filter": scope_filter.clone(),
            }),
        ) {
            return rbac_result;
        }

        let store = FileMemoryStore::new_with_embedding_provider(
            self.policy.memory_state_dir.clone(),
            self.policy.memory_embedding_provider_config(),
        );

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "memory_read",
            json!({
                "memory_id": memory_id.clone(),
                "scope_filter": scope_filter.clone(),
                "store_root": self.policy.memory_state_dir.display().to_string(),
                "storage_backend": store.storage_backend_label(),
            }),
        ) {
            return rate_limit_result;
        }
        match store.read_entry(memory_id.as_str(), scope_filter.as_ref()) {
            Ok(Some(record)) => ToolExecutionResult::ok(json!({
                "tool": "memory_read",
                "found": true,
                "memory_id": record.entry.memory_id,
                "scope": record.scope,
                "summary": record.entry.summary,
                "memory_type": record.memory_type.as_str(),
                "importance": record.importance,
                "tags": record.entry.tags,
                "facts": record.entry.facts,
                "source_event_key": record.entry.source_event_key,
                "recency_weight_bps": record.entry.recency_weight_bps,
                "confidence_bps": record.entry.confidence_bps,
                "updated_unix_ms": record.updated_unix_ms,
                "last_accessed_at_unix_ms": record.last_accessed_at_unix_ms,
                "access_count": record.access_count,
                "forgotten": record.forgotten,
                "embedding_source": record.embedding_source,
                "embedding_model": record.embedding_model,
                "embedding_reason_code": record.embedding_reason_code,
                "relations": record.relations,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Ok(None) => ToolExecutionResult::ok(json!({
                "tool": "memory_read",
                "found": false,
                "memory_id": memory_id,
                "scope_filter": scope_filter.clone(),
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "tool": "memory_read",
                "reason_code": "memory_backend_error",
                "memory_id": memory_id,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `MemoryDeleteTool` used across Tau components.
pub struct MemoryDeleteTool {
    policy: Arc<ToolPolicy>,
}

impl MemoryDeleteTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for MemoryDeleteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_delete".to_string(),
            description: "Soft-delete a scoped semantic memory entry by id".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory id to soft-delete" },
                    "workspace_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "actor_id": { "type": "string" }
                },
                "required": ["memory_id"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let memory_id = match required_string(&arguments, "memory_id") {
            Ok(memory_id) => memory_id,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_delete",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let scope_filter = memory_scope_filter_from_arguments(&arguments);
        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "memory_delete",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "memory_id": memory_id.clone(),
                "scope_filter": scope_filter.clone(),
            }),
        ) {
            return rbac_result;
        }

        let store = FileMemoryStore::new_with_embedding_provider(
            self.policy.memory_state_dir.clone(),
            self.policy.memory_embedding_provider_config(),
        );
        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "memory_delete",
            json!({
                "memory_id": memory_id.clone(),
                "scope_filter": scope_filter.clone(),
                "store_root": self.policy.memory_state_dir.display().to_string(),
                "storage_backend": store.storage_backend_label(),
            }),
        ) {
            return rate_limit_result;
        }

        match store.soft_delete_entry(memory_id.as_str(), scope_filter.as_ref()) {
            Ok(Some(record)) => ToolExecutionResult::ok(json!({
                "tool": "memory_delete",
                "deleted": true,
                "reason_code": "memory_deleted",
                "memory_id": record.entry.memory_id,
                "scope": record.scope,
                "updated_unix_ms": record.updated_unix_ms,
                "last_accessed_at_unix_ms": record.last_accessed_at_unix_ms,
                "access_count": record.access_count,
                "forgotten": record.forgotten,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Ok(None) => ToolExecutionResult::error(json!({
                "tool": "memory_delete",
                "reason_code": "memory_not_found",
                "memory_id": memory_id,
                "scope_filter": scope_filter,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "tool": "memory_delete",
                "reason_code": "memory_backend_error",
                "memory_id": memory_id,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `MemorySearchTool` used across Tau components.
pub struct MemorySearchTool {
    policy: Arc<ToolPolicy>,
}

impl MemorySearchTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search semantic memory entries using deterministic similarity ranking"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Semantic search query" },
                    "workspace_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "actor_id": { "type": "string" },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum matches to return (default {}, max {})",
                            self.policy.memory_search_default_limit,
                            self.policy.memory_search_max_limit
                        )
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let query = match required_string(&arguments, "query") {
            Ok(query) => query,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_search",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        if query.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "tool": "memory_search",
                "reason_code": "memory_empty_query",
                "error": "query must not be empty",
            }));
        }
        let limit = match optional_usize(
            &arguments,
            "limit",
            self.policy.memory_search_default_limit,
            self.policy
                .memory_search_max_limit
                .max(self.policy.memory_search_default_limit),
        ) {
            Ok(limit) => limit,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "memory_search",
                    "reason_code": "memory_invalid_arguments",
                    "error": error,
                }))
            }
        };
        let scope = memory_scope_filter_from_arguments(&arguments).unwrap_or_default();

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "memory_search",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "query": query.clone(),
                "limit": limit,
                "scope_filter": scope.clone(),
            }),
        ) {
            return rbac_result;
        }

        let store = FileMemoryStore::new_with_embedding_provider(
            self.policy.memory_state_dir.clone(),
            self.policy.memory_embedding_provider_config(),
        );

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "memory_search",
            json!({
                "query": query.clone(),
                "limit": limit,
                "scope_filter": scope.clone(),
                "store_root": self.policy.memory_state_dir.display().to_string(),
                "storage_backend": store.storage_backend_label(),
            }),
        ) {
            return rate_limit_result;
        }
        match store.search(
            query.as_str(),
            &MemorySearchOptions {
                scope,
                limit,
                embedding_dimensions: self.policy.memory_embedding_dimensions,
                min_similarity: self.policy.memory_min_similarity,
                enable_hybrid_retrieval: self.policy.memory_enable_hybrid_retrieval,
                bm25_k1: self.policy.memory_bm25_k1,
                bm25_b: self.policy.memory_bm25_b,
                bm25_min_score: self.policy.memory_bm25_min_score,
                rrf_k: self.policy.memory_rrf_k,
                rrf_vector_weight: self.policy.memory_rrf_vector_weight,
                rrf_lexical_weight: self.policy.memory_rrf_lexical_weight,
                graph_signal_weight: MEMORY_GRAPH_SIGNAL_WEIGHT,
                enable_embedding_migration: self.policy.memory_enable_embedding_migration,
                benchmark_against_hash: self.policy.memory_benchmark_against_hash,
                benchmark_against_vector_only: self.policy.memory_benchmark_against_vector_only,
            },
        ) {
            Ok(result) => ToolExecutionResult::ok(json!({
                "tool": "memory_search",
                "query": result.query,
                "limit": limit,
                "scanned": result.scanned,
                "returned": result.returned,
                "retrieval_backend": result.retrieval_backend,
                "retrieval_reason_code": result.retrieval_reason_code,
                "min_similarity": self.policy.memory_min_similarity,
                "embedding_dimensions": self.policy.memory_embedding_dimensions,
                "embedding_backend": result.embedding_backend,
                "embedding_reason_code": result.embedding_reason_code,
                "migrated_entries": result.migrated_entries,
                "query_embedding_latency_ms": result.query_embedding_latency_ms,
                "ranking_latency_ms": result.ranking_latency_ms,
                "lexical_ranking_latency_ms": result.lexical_ranking_latency_ms,
                "fusion_latency_ms": result.fusion_latency_ms,
                "vector_candidates": result.vector_candidates,
                "lexical_candidates": result.lexical_candidates,
                "baseline_hash_overlap": result.baseline_hash_overlap,
                "baseline_vector_overlap": result.baseline_vector_overlap,
                "matches": result.matches,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "tool": "memory_search",
                "reason_code": "memory_backend_error",
                "query": query,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `MemoryTreeTool` used across Tau components.
pub struct MemoryTreeTool {
    policy: Arc<ToolPolicy>,
}

impl MemoryTreeTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for MemoryTreeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_tree".to_string(),
            description: "Render memory store hierarchy (workspace -> channel -> actor)"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, _arguments: Value) -> ToolExecutionResult {
        let store = FileMemoryStore::new_with_embedding_provider(
            self.policy.memory_state_dir.clone(),
            self.policy.memory_embedding_provider_config(),
        );
        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "memory_tree",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "store_root": self.policy.memory_state_dir.display().to_string(),
            }),
        ) {
            return rbac_result;
        }
        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "memory_tree",
            json!({
                "store_root": self.policy.memory_state_dir.display().to_string(),
                "storage_backend": store.storage_backend_label(),
            }),
        ) {
            return rate_limit_result;
        }
        match store.tree() {
            Ok(tree) => ToolExecutionResult::ok(json!({
                "tool": "memory_tree",
                "total_entries": tree.total_entries,
                "workspaces": tree.workspaces,
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "tool": "memory_tree",
                "reason_code": "memory_backend_error",
                "store_root": store.root_dir().display().to_string(),
                "storage_backend": store.storage_backend_label(),
                "backend_reason_code": store.storage_backend_reason_code(),
                "storage_path": store
                    .storage_path()
                    .map(|path| path.display().to_string()),
                "error": error.to_string(),
            })),
        }
    }
}
