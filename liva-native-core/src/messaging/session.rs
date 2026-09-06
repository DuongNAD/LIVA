//! Session & Context Isolation Router
//!
//! Provides thread-safe, isolated conversation session context management,
//! memory scope scoping (Ephemeral, Working, Persistent, VaultBound), eviction,
//! and routing mechanics.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::normalized::ChannelId;

/// Unique identifier for conversation sessions in LIVA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Generate a new random SessionId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct from an existing Uuid.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Access the underlying Uuid.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(SessionId)
    }
}

/// Scoped memory isolation policy for session threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Discarded immediately after action / task completion (e.g. tool probe, scraping).
    Ephemeral,
    /// In-memory working context, evictable after idle timeout.
    Working,
    /// Persisted across restarts into SQLite WAL memory / RAG vector ledger.
    Persistent,
    /// Vault-bound encrypted context isolated from standard history recall.
    VaultBound,
}

impl MemoryScope {
    /// Whether this scope should be persisted to long-term storage.
    pub fn is_persisted(&self) -> bool {
        matches!(self, Self::Persistent | Self::VaultBound)
    }

    /// Whether this scope is strictly ephemeral.
    pub fn is_ephemeral(&self) -> bool {
        matches!(self, Self::Ephemeral)
    }

    /// Canonical name of the memory scope.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Working => "working",
            Self::Persistent => "persistent",
            Self::VaultBound => "vault_bound",
        }
    }
}

/// Lifecycle state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Idle,
    Suspended,
    Terminated,
}

impl SessionState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Terminated)
    }
}

/// Context state representing an isolated multi-turn interaction thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: SessionId,
    pub channel: ChannelId,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub state: SessionState,
    pub memory_scope: MemoryScope,
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_skill_id: Option<String>,
}

impl SessionContext {
    /// Create a new session context.
    pub fn new(
        channel: ChannelId,
        user_id: impl Into<String>,
        thread_id: Option<String>,
        memory_scope: MemoryScope,
    ) -> Self {
        let now = Utc::now();
        Self {
            session_id: SessionId::new(),
            channel,
            user_id: user_id.into(),
            thread_id,
            created_at: now,
            last_active_at: now,
            state: SessionState::Active,
            memory_scope,
            variables: HashMap::new(),
            active_skill_id: None,
        }
    }

    /// Touch the session timestamp to refresh idle timeout.
    pub fn touch(&mut self) {
        self.last_active_at = Utc::now();
        if self.state == SessionState::Idle {
            self.state = SessionState::Active;
        }
    }

    /// Check if session has exceeded the specified TTL duration.
    pub fn is_expired(&self, ttl: Duration) -> bool {
        let chrono_ttl = ChronoDuration::from_std(ttl).unwrap_or(ChronoDuration::MAX);
        Utc::now().signed_duration_since(self.last_active_at) > chrono_ttl
    }

    /// Set a context variable.
    pub fn set_variable(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.variables.insert(key.into(), value);
        self.touch();
    }

    /// Retrieve a context variable reference.
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }

    /// Remove a context variable.
    pub fn remove_variable(&mut self, key: &str) -> Option<serde_json::Value> {
        self.touch();
        self.variables.remove(key)
    }

    /// Update session lifecycle state.
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
        self.touch();
    }
}

/// Errors occurring during session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(SessionId),
    #[error("Session lock conflict: {0}")]
    LockConflict(String),
    #[error("Session serialization error: {0}")]
    Serialization(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Invalid memory scope: {0}")]
    InvalidScope(String),
}

/// Trait defining the lifecycle management of isolated sessions.
#[async_trait::async_trait]
pub trait SessionManager: Send + Sync {
    /// Retrieve an existing session by channel, user, and thread ID, or create a new one.
    async fn get_or_create_session(
        &self,
        channel: &ChannelId,
        user_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Arc<RwLock<SessionContext>>, SessionError>;

    /// Retrieve an active session by SessionId.
    async fn get_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<Arc<RwLock<SessionContext>>>, SessionError>;

    /// Refresh session active timestamp.
    async fn touch_session(&self, session_id: &SessionId) -> Result<(), SessionError>;

    /// Evict all sessions whose inactivity exceeds the provided TTL.
    async fn evict_expired(&self, ttl: Duration) -> Result<usize, SessionError>;

    /// Persist session state into durable backend if scope is persistent.
    async fn persist_session(&self, session_id: &SessionId) -> Result<(), SessionError>;

    /// Terminate and remove a session.
    async fn terminate_session(&self, session_id: &SessionId) -> Result<(), SessionError>;

    /// List snapshots of all tracked sessions.
    async fn list_sessions(&self) -> Result<Vec<SessionContext>, SessionError>;
}

/// High-performance in-memory session manager with fine-grained read-write locks.
pub struct InMemorySessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<RwLock<SessionContext>>>>,
    channel_index: RwLock<HashMap<(ChannelId, String, Option<String>), SessionId>>,
    default_scope: MemoryScope,
}

impl InMemorySessionManager {
    /// Create a new in-memory session manager with default memory scope.
    pub fn new(default_scope: MemoryScope) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            channel_index: RwLock::new(HashMap::new()),
            default_scope,
        }
    }

    /// Parse route identifier string ("main", "isolated", "session:<uuid>").
    pub fn resolve_route(&self, route_key: &str) -> (Option<SessionId>, MemoryScope) {
        let trimmed = route_key.trim();
        if trimmed == "main" {
            (None, MemoryScope::Persistent)
        } else if trimmed == "isolated" {
            (None, MemoryScope::Ephemeral)
        } else if let Some(stripped) = trimmed.strip_prefix("session:") {
            if let Ok(id) = SessionId::from_str(stripped) {
                (Some(id), MemoryScope::Persistent)
            } else {
                (None, MemoryScope::Working)
            }
        } else if let Ok(id) = SessionId::from_str(trimmed) {
            (Some(id), MemoryScope::Persistent)
        } else {
            (None, self.default_scope)
        }
    }
}

impl Default for InMemorySessionManager {
    fn default() -> Self {
        Self::new(MemoryScope::Working)
    }
}

#[async_trait::async_trait]
impl SessionManager for InMemorySessionManager {
    async fn get_or_create_session(
        &self,
        channel: &ChannelId,
        user_id: &str,
        thread_id: Option<&str>,
    ) -> Result<Arc<RwLock<SessionContext>>, SessionError> {
        let key = (channel.clone(), user_id.to_string(), thread_id.map(ToString::to_string));
        
        // Fast path: check index with read lock, dropping each lock before the next
        {
            let session_id = {
                let index = self.channel_index.read().await;
                index.get(&key).copied()
            };
            if let Some(session_id) = session_id {
                let ctx_opt = {
                    let sessions = self.sessions.read().await;
                    sessions.get(&session_id).cloned()
                };
                if let Some(ctx_arc) = ctx_opt {
                    {
                        let mut ctx = ctx_arc.write().await;
                        ctx.touch();
                    }
                    return Ok(ctx_arc);
                }
            }
        }

        // Slow path: acquire sessions write lock FIRST (Level 1), then channel_index write lock (Level 2)
        let mut sessions = self.sessions.write().await;
        let mut index = self.channel_index.write().await;

        // Re-check after acquiring write locks
        if let Some(session_id) = index.get(&key) {
            if let Some(ctx_arc) = sessions.get(session_id).cloned() {
                drop(index);
                drop(sessions);
                {
                    let mut ctx = ctx_arc.write().await;
                    ctx.touch();
                }
                return Ok(ctx_arc);
            }
        }

        let new_ctx = SessionContext::new(
            channel.clone(),
            user_id,
            thread_id.map(ToString::to_string),
            self.default_scope,
        );
        let session_id = new_ctx.session_id;
        let ctx_arc = Arc::new(RwLock::new(new_ctx));

        sessions.insert(session_id, ctx_arc.clone());
        index.insert(key, session_id);

        Ok(ctx_arc)
    }

    async fn get_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<Arc<RwLock<SessionContext>>>, SessionError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn touch_session(&self, session_id: &SessionId) -> Result<(), SessionError> {
        let ctx_arc = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).cloned()
        };
        if let Some(ctx_arc) = ctx_arc {
            let mut ctx = ctx_arc.write().await;
            ctx.touch();
            Ok(())
        } else {
            Err(SessionError::NotFound(*session_id))
        }
    }

    async fn evict_expired(&self, ttl: Duration) -> Result<usize, SessionError> {
        let mut to_evict = Vec::new();
        {
            let sessions = self.sessions.read().await;
            for (id, ctx_arc) in sessions.iter() {
                let ctx = ctx_arc.read().await;
                // Ephemeral and Working scopes are evictable by TTL; Persistent/VaultBound remain
                if (ctx.memory_scope.is_ephemeral() || ctx.memory_scope == MemoryScope::Working)
                    && ctx.is_expired(ttl)
                {
                    let key = (ctx.channel.clone(), ctx.user_id.clone(), ctx.thread_id.clone());
                    to_evict.push((*id, key));
                }
            }
        }

        if to_evict.is_empty() {
            return Ok(0);
        }

        // Lock hierarchy: sessions (Level 1) then channel_index (Level 2)
        let mut sessions = self.sessions.write().await;
        let mut index = self.channel_index.write().await;
        let count = to_evict.len();

        for (id, key) in to_evict {
            sessions.remove(&id);
            index.remove(&key);
        }

        Ok(count)
    }

    async fn persist_session(&self, session_id: &SessionId) -> Result<(), SessionError> {
        let ctx_arc = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).cloned()
        };
        if let Some(ctx_arc) = ctx_arc {
            let mut ctx = ctx_arc.write().await;
            if !ctx.memory_scope.is_persisted() {
                ctx.memory_scope = MemoryScope::Persistent;
            }
            ctx.touch();
            Ok(())
        } else {
            Err(SessionError::NotFound(*session_id))
        }
    }

    async fn terminate_session(&self, session_id: &SessionId) -> Result<(), SessionError> {
        // Lock hierarchy: sessions (Level 1) then channel_index (Level 2)
        let mut sessions = self.sessions.write().await;
        let mut index = self.channel_index.write().await;

        if let Some(ctx_arc) = sessions.remove(session_id) {
            let key = {
                let mut ctx = ctx_arc.write().await;
                ctx.set_state(SessionState::Terminated);
                (ctx.channel.clone(), ctx.user_id.clone(), ctx.thread_id.clone())
            };
            index.remove(&key);
            Ok(())
        } else {
            Err(SessionError::NotFound(*session_id))
        }
    }

    async fn list_sessions(&self) -> Result<Vec<SessionContext>, SessionError> {
        let session_arcs: Vec<Arc<RwLock<SessionContext>>> = {
            let sessions = self.sessions.read().await;
            sessions.values().cloned().collect()
        };
        let mut results = Vec::with_capacity(session_arcs.len());
        for ctx_arc in session_arcs {
            let ctx = ctx_arc.read().await;
            results.push(ctx.clone());
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation_and_variable_isolation() {
        let mgr = InMemorySessionManager::new(MemoryScope::Working);
        let s1 = mgr
            .get_or_create_session(&ChannelId::Telegram, "user_1", None)
            .await
            .expect("create session 1");
        let s2 = mgr
            .get_or_create_session(&ChannelId::Discord, "user_2", None)
            .await
            .expect("create session 2");

        let id1 = s1.read().await.session_id;
        let id2 = s2.read().await.session_id;
        assert_ne!(id1, id2);

        // Variables in s1 should not be visible in s2
        s1.write()
            .await
            .set_variable("auth_code", serde_json::json!("123456"));
        assert_eq!(
            s1.read().await.get_variable("auth_code"),
            Some(&serde_json::json!("123456"))
        );
        assert_eq!(s2.read().await.get_variable("auth_code"), None);
    }

    #[tokio::test]
    async fn test_session_idempotent_get_or_create() {
        let mgr = InMemorySessionManager::new(MemoryScope::Working);
        let s1 = mgr
            .get_or_create_session(&ChannelId::Slack, "user_alice", Some("thread_100"))
            .await
            .expect("get or create 1");
        let s2 = mgr
            .get_or_create_session(&ChannelId::Slack, "user_alice", Some("thread_100"))
            .await
            .expect("get or create 2");

        assert_eq!(s1.read().await.session_id, s2.read().await.session_id);
    }

    #[tokio::test]
    async fn test_session_eviction_lifecycle() {
        let mgr = InMemorySessionManager::new(MemoryScope::Ephemeral);
        let s1 = mgr
            .get_or_create_session(&ChannelId::WebSocketWidget, "client_01", None)
            .await
            .expect("create session");
        let id1 = s1.read().await.session_id;

        // Force expiration
        {
            let mut ctx = s1.write().await;
            ctx.last_active_at = Utc::now() - ChronoDuration::seconds(60);
        }

        let evicted = mgr.evict_expired(Duration::from_secs(10)).await.expect("evict");
        assert_eq!(evicted, 1);

        let lookup = mgr.get_session(&id1).await.expect("lookup");
        assert!(lookup.is_none());
    }

    #[tokio::test]
    async fn test_route_resolver() {
        let mgr = InMemorySessionManager::default();
        let (id_main, scope_main) = mgr.resolve_route("main");
        assert_eq!(id_main, None);
        assert_eq!(scope_main, MemoryScope::Persistent);

        let (id_iso, scope_iso) = mgr.resolve_route("isolated");
        assert_eq!(id_iso, None);
        assert_eq!(scope_iso, MemoryScope::Ephemeral);

        let random_uuid = Uuid::new_v4();
        let (id_custom, scope_custom) = mgr.resolve_route(&format!("session:{}", random_uuid));
        assert_eq!(id_custom, Some(SessionId::from_uuid(random_uuid)));
        assert_eq!(scope_custom, MemoryScope::Persistent);
    }

    #[tokio::test]
    async fn test_terminate_session() {
        let mgr = InMemorySessionManager::new(MemoryScope::Working);
        let s = mgr
            .get_or_create_session(&ChannelId::LocalCli, "local_user", None)
            .await
            .expect("create");
        let id = s.read().await.session_id;

        mgr.terminate_session(&id).await.expect("terminate");
        let lookup = mgr.get_session(&id).await.expect("lookup");
        assert!(lookup.is_none());
    }
}
