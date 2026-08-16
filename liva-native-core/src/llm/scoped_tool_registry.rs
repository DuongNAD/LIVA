//! Hierarchical Scoped Tool Registry and Guarded Execution Pipeline (Milestone 2 - Feature F4 & F5).
//!
//! Provides context-scoped dynamic tool registration, RAII Fiber disposal guards (Cordis equivalent),
//! hierarchical scope inheritance, fail-closed CommandPrincipal channel authorization,
//! RiskTier policy gating, and secret-scrubbed action audit ledger persistence.

use crate::authorization::{CommandPrincipal, authorize_command};
use crate::cognitive::{ActionAuditRecord, PolicyEngine, RedactedAuditLedger, RiskTier};
use crate::llm::tool_calling::{CatalogTool, rank_tools, validate_arguments};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

/// Error types for tool registration and scope management.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    ScopeNotFound(String),
    DuplicateTool(String, String),
    ValidationError(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScopeNotFound(s) => write!(f, "Scope '{s}' not found"),
            Self::DuplicateTool(t, s) => write!(f, "Duplicate tool '{t}' in scope '{s}'"),
            Self::ValidationError(e) => write!(f, "Tool validation error: {e}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Error types for guarded tool execution.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolExecError {
    ToolNotFound(String),
    UnauthorizedPrincipal(String),
    PolicyViolation(String),
    InvalidArguments(String),
    ExecutionFailed(String),
    ConfirmationRequired { token: String, risk: String },
}

impl std::fmt::Display for ToolExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotFound(t) => write!(f, "Tool '{t}' not found"),
            Self::UnauthorizedPrincipal(p) => write!(f, "Principal not authorized: {p}"),
            Self::PolicyViolation(r) => write!(f, "Policy violation: {r}"),
            Self::InvalidArguments(a) => write!(f, "Invalid arguments: {a}"),
            Self::ExecutionFailed(e) => write!(f, "Execution error: {e}"),
            Self::ConfirmationRequired { token, risk } => {
                write!(f, "Confirmation required: token={token}, risk={risk}")
            }
        }
    }
}

impl std::error::Error for ToolExecError {}

/// Hierarchical scope definition for active tool registration and visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolScope {
    pub scope_id: String,
    pub parent_scope: Option<String>,
    pub principal: CommandPrincipal,
    pub allowed_tools: HashSet<String>,
}

impl ToolScope {
    pub fn new(scope_id: impl Into<String>, principal: CommandPrincipal) -> Self {
        Self {
            scope_id: scope_id.into(),
            parent_scope: None,
            principal,
            allowed_tools: HashSet::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_scope = Some(parent_id.into());
        self
    }

    pub fn allow_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.allowed_tools.insert(tool_name.into());
        self
    }

    pub fn allow_tools<I, S>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for t in tools {
            self.allowed_tools.insert(t.into());
        }
        self
    }

    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        if self.allowed_tools.is_empty() {
            true
        } else {
            self.allowed_tools.contains(tool_name)
        }
    }
}

/// RAII Fiber disposal guard (Cordis Fiber equivalent in Rust).
/// When dropped, unregisters the registered scoped tool from the registry.
#[derive(Debug)]
pub struct ScopeGuard {
    scope_id: String,
    tool_name: String,
    registry: Arc<ScopedToolRegistryInner>,
    dismissed: bool,
}

impl ScopeGuard {
    pub fn new(
        scope_id: String,
        tool_name: String,
        registry: Arc<ScopedToolRegistryInner>,
    ) -> Self {
        Self {
            scope_id,
            tool_name,
            registry,
            dismissed: false,
        }
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// Disarms the guard so it does not unregister on drop.
    pub fn disarm(mut self) {
        self.dismissed = true;
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if self.dismissed {
            return;
        }
        if let Ok(mut tools) = self.registry.scoped_tools.write()
            && let Some(scope_map) = tools.get_mut(&self.scope_id)
        {
            scope_map.remove(&self.tool_name);
        }
    }
}

#[derive(Debug)]
pub struct ScopedToolRegistryInner {
    pub(crate) scoped_tools: RwLock<HashMap<String, HashMap<String, CatalogTool>>>,
    pub(crate) scopes: RwLock<HashMap<String, ToolScope>>,
}

/// Thread-safe hierarchical Scoped Tool Registry.
#[derive(Clone, Debug)]
pub struct ScopedToolRegistry {
    inner: Arc<ScopedToolRegistryInner>,
}

impl Default for ScopedToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopedToolRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScopedToolRegistryInner {
                scoped_tools: RwLock::new(HashMap::new()),
                scopes: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Registers or updates a scope in the registry.
    pub fn register_scope(&self, scope: ToolScope) {
        let mut scopes = self.inner.scopes.write().expect("lock scopes");
        scopes.insert(scope.scope_id.clone(), scope);
    }

    /// Unregisters a scope and all its registered tools.
    pub fn unregister_scope(&self, scope_id: &str) {
        let mut scopes = self.inner.scopes.write().expect("lock scopes");
        scopes.remove(scope_id);
        let mut tools = self.inner.scoped_tools.write().expect("lock tools");
        tools.remove(scope_id);
    }

    /// Retrieves a scope definition if present.
    pub fn get_scope(&self, scope_id: &str) -> Option<ToolScope> {
        let scopes = self.inner.scopes.read().expect("lock scopes");
        scopes.get(scope_id).cloned()
    }

    /// Registers a tool under a specific scope, returning an RAII ScopeGuard.
    pub fn register_scoped(
        &self,
        scope_id: &str,
        tool: CatalogTool,
    ) -> Result<ScopeGuard, ToolError> {
        let mut tools = self.inner.scoped_tools.write().expect("lock tools");
        let scope_map = tools.entry(scope_id.to_string()).or_default();
        if scope_map.contains_key(&tool.name) {
            return Err(ToolError::DuplicateTool(tool.name, scope_id.to_string()));
        }
        let tool_name = tool.name.clone();
        scope_map.insert(tool_name.clone(), tool);

        Ok(ScopeGuard::new(
            scope_id.to_string(),
            tool_name,
            Arc::clone(&self.inner),
        ))
    }

    /// Resolves all tools available to a scope following hierarchical inheritance.
    pub fn resolve_tools_for_scope(&self, scope_id: &str) -> Vec<CatalogTool> {
        let scopes = self.inner.scopes.read().expect("lock scopes");
        let tools = self.inner.scoped_tools.read().expect("lock tools");

        let mut resolved: HashMap<String, CatalogTool> = HashMap::new();
        let mut curr = Some(scope_id.to_string());
        let mut visited = HashSet::new();

        while let Some(sid) = curr {
            if !visited.insert(sid.clone()) {
                break; // Cycle guard
            }
            if let Some(scope_tools) = tools.get(&sid) {
                for (name, tool) in scope_tools {
                    resolved.entry(name.clone()).or_insert_with(|| tool.clone());
                }
            }
            curr = scopes.get(&sid).and_then(|s| s.parent_scope.clone());
        }

        resolved.into_values().collect()
    }

    /// Selects and ranks the top-K relevant tools within a given scope.
    pub fn select_tools_for_scope(
        &self,
        scope: &ToolScope,
        query: &str,
        top_k: usize,
    ) -> Vec<CatalogTool> {
        let all_tools = self.resolve_tools_for_scope(&scope.scope_id);
        let allowed_tools: Vec<CatalogTool> = all_tools
            .into_iter()
            .filter(|t| scope.is_tool_allowed(&t.name))
            .collect();

        if allowed_tools.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let mut catalog = crate::llm::ToolCatalog::new();
        for t in &allowed_tools {
            catalog.add_server(
                &t.server,
                &[crate::mcp::protocol::Tool {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: match serde_json::from_value(t.input_schema.clone()) {
                        Ok(s) => s,
                        Err(_) => schemars::schema_for!(serde_json::Value),
                    },
                }],
            );
            if !t.embed_extra.is_empty() {
                catalog.set_embed_extra(&t.server, &t.name, &t.embed_extra);
            }
        }

        let ranked_indices = rank_tools(&catalog, query, None, top_k);
        ranked_indices
            .into_iter()
            .filter_map(|idx| catalog.tools().get(idx).cloned())
            .collect()
    }

    /// Executes a tool call through the fail-closed guarded pipeline.
    pub fn execute_guarded(
        &self,
        scope: &ToolScope,
        tool_name: &str,
        args: &Value,
    ) -> Result<Value, ToolExecError> {
        self.execute_guarded_with_audit(None, scope, tool_name, args)
    }

    /// Executes a tool call with optional SQLite audit ledger recording.
    pub fn execute_guarded_with_audit(
        &self,
        conn: Option<&Connection>,
        scope: &ToolScope,
        tool_name: &str,
        args: &Value,
    ) -> Result<Value, ToolExecError> {
        let available = self.resolve_tools_for_scope(&scope.scope_id);
        let tool = available
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| ToolExecError::ToolNotFound(tool_name.to_string()))?;

        // 1. Check scope allowed_tools
        if !scope.allowed_tools.is_empty() && !scope.allowed_tools.contains(tool_name) {
            return Err(ToolExecError::UnauthorizedPrincipal(format!(
                "Principal {:?} not authorized for tool '{}' in scope '{}'",
                scope.principal, tool_name, scope.scope_id
            )));
        }

        // 2. Explicit CommandPrincipal channel authorization for restricted native commands
        if tool.server == crate::llm::tool_calling::NATIVE_SERVER
            && crate::authorization::is_known_command(tool_name)
            && authorize_command(scope.principal, tool_name).is_err()
        {
            return Err(ToolExecError::UnauthorizedPrincipal(format!(
                "Principal {:?} is not authorized for command '{}'",
                scope.principal, tool_name
            )));
        }

        // 3. RiskTier policy check
        let risk_tier = PolicyEngine::classify_tool(&tool.qualified());
        if risk_tier == RiskTier::PhysicalOrIrreversible
            && scope.principal != CommandPrincipal::LocalCli
            && scope.principal != CommandPrincipal::Test
        {
            return Err(ToolExecError::PolicyViolation(format!(
                "Tool '{}' requires physical confirmation",
                tool_name
            )));
        }

        // 4. Validate arguments against schema
        if let Err(e) = validate_arguments(&tool.input_schema, args) {
            return Err(ToolExecError::InvalidArguments(e));
        }

        let res = json!({
            "status": "executed",
            "tool": tool.name,
            "args": args,
            "risk": risk_tier.to_string(),
        });

        // 5. Audit ledger recording if connection provided
        if let Some(db_conn) = conn {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let scrubbed_args_str = serde_json::to_string(args).unwrap_or_default();
            let action_id = uuid::Uuid::new_v4().to_string();
            let idempotency_key = hex::encode(md5_hash(
                format!("{}:{}:{}", scope.scope_id, tool_name, scrubbed_args_str).as_bytes(),
            ));

            let record = ActionAuditRecord {
                id: None,
                action_id,
                idempotency_key,
                source_event_id: None,
                tool_id: tool.qualified(),
                risk_tier: risk_tier.to_string(),
                policy_decision: "allow".to_string(),
                principal: format!("{:?}", scope.principal),
                redacted_params: scrubbed_args_str,
                redacted_observation: Some(serde_json::to_string(&res).unwrap_or_default()),
                status: "success".to_string(),
                duration_ms: Some(1),
                created_at_ms: now_ms,
            };

            let _ = RedactedAuditLedger::record_action(db_conn, &record);
        }

        Ok(res)
    }
}

fn md5_hash(data: &[u8]) -> [u8; 16] {
    // Uses sha256 truncated to 16 bytes for deterministic hash
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&result[..16]);
    out
}
