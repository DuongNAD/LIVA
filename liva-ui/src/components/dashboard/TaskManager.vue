<script setup lang="ts">
/**
 * TaskManager.vue — Persistent Task Management + Inline AI Planning
 * ==================================================================
 * Full CRUD synced with Gateway SQLite backend.
 * [v25] Inline Planning Chat: When creating a task with description,
 *       AI auto-asks clarifying questions within the Dashboard.
 *       User answers inline → AI auto-updates the task with a detailed plan.
 */
import { computed, onActivated, ref, nextTick, onMounted } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

const gateway = useGateway();
const { t } = useI18n();
const tasks = computed(() => gateway.tasksList.value || []);

// Stats
const stats = computed(() => {
  const all = tasks.value;
  return {
    total: all.length,
    pending: all.filter((t: any) => t.status === 'pending').length,
    inProgress: all.filter((t: any) => t.status === 'in-progress').length,
    done: all.filter((t: any) => t.status === 'done').length,
  };
});

// Filter
const _filter = ref<'all' | 'pending' | 'in-progress' | 'done'>('all');

const filteredTasks = computed(() => {
  if (_filter.value === 'all') return tasks.value;
  return tasks.value.filter((t: any) => t.status === _filter.value);
});

// New task form
const newTitle = ref('');
const newDesc = ref('');
const newPriority = ref<'low' | 'medium' | 'high'>('medium');
const showForm = ref(false);

// ═══════════════════════════════════════════
// [v25] Inline Planning Chat State
// ═══════════════════════════════════════════
interface PlanMessage {
  role: 'user' | 'ai';
  content: string;
}

const planChats = ref<Record<string, PlanMessage[]>>({});
const activePlanId = ref<string | null>(null);
const planInput = ref('');
const planLoading = ref(false);
const chatContainerRef = ref<HTMLElement | null>(null);

// Subscribe to AI planning replies
gateway.onTaskPlanReply((payload) => {
  const { taskId, message, done } = payload;
  if (!planChats.value[taskId]) planChats.value[taskId] = [];
  planChats.value[taskId].push({ role: 'ai', content: message });
  planLoading.value = false;
  
  if (done) {
    // Delay close so user can read the final plan
    setTimeout(() => {
      activePlanId.value = null;
      delete planChats.value[taskId];
      gateway.sendMsg('get_tasks'); // Refresh to get updated description
    }, 3000);
  }
  
  // Auto-scroll chat to bottom
  nextTick(() => scrollChatToBottom());
});

const scrollChatToBottom = () => {
  if (chatContainerRef.value) {
    chatContainerRef.value.scrollTop = chatContainerRef.value.scrollHeight;
  }
};

const sendPlanMessage = () => {
  if (!planInput.value.trim() || !activePlanId.value || planLoading.value) return;
  
  const msg = planInput.value.trim();
  const taskId = activePlanId.value;
  
  if (!planChats.value[taskId]) planChats.value[taskId] = [];
  planChats.value[taskId].push({ role: 'user', content: msg });
  
  planLoading.value = true;
  gateway.sendMsg('task_plan_chat', { taskId, message: msg });
  planInput.value = '';
  
  nextTick(() => scrollChatToBottom());
};

const startPlanning = (task: any) => {
  activePlanId.value = task.id;
  if (!planChats.value[task.id]) planChats.value[task.id] = [];
};

const closePlanning = () => {
  activePlanId.value = null;
  planLoading.value = false;
};

// ═══════════════════════════════════════════

const addTask = () => {
  if (!newTitle.value.trim()) return;
  
  const title = newTitle.value.trim();
  const desc = newDesc.value.trim();
  
  gateway.sendMsg('add_task', {
    title,
    description: desc,
    priority: newPriority.value,
  });
  
  newTitle.value = '';
  newDesc.value = '';
  newPriority.value = 'medium';
  showForm.value = false;
  
  // If description was provided, auto-open planning chat after a brief delay
  // (wait for task to be created and AI to respond)
  if (desc) {
    planLoading.value = true;
    setTimeout(() => {
      // Find the newest task (should be the one we just created)
      const latest = tasks.value[0]; // tasks sorted by created_at DESC
      if (latest) {
        activePlanId.value = latest.id;
        if (!planChats.value[latest.id]) planChats.value[latest.id] = [];
        planChats.value[latest.id].push({ role: 'user', content: desc });
      }
    }, 500);
  }
};

const quickAdd = (title: string) => {
  gateway.sendMsg('add_task', { title, priority: 'medium' });
};

const executeTask = (task: any) => {
  // Thay vì ném task sang widget chat chính gây mất tập trung,
  // chúng ta mở luôn khung chat nội bộ để AI hướng dẫn thực hiện.
  startPlanning(task);
  
  const msg = t('tm_ai_plan_start');
  planChats.value[task.id].push({ role: 'user', content: msg });
  planLoading.value = true;
  
  gateway.sendMsg('task_plan_chat', { taskId: task.id, message: msg });
  gateway.sendMsg('update_task', { id: task.id, updates: { status: 'in-progress' } });
};

const completeTask = (task: any) => {
  gateway.sendMsg('update_task', { id: task.id, updates: { status: 'done' } });
};

const deleteTask = (id: string) => {
  if (activePlanId.value === id) closePlanning();
  gateway.sendMsg('delete_task', { id });
};

const statusIcon = (s: string) => s === 'done' ? '✅' : s === 'in-progress' ? '⏳' : '⏹️';
const priorityBadge = (p: string) => p === 'high' ? 'badge-danger' : p === 'medium' ? 'badge-warning' : 'badge-info';
const fmtDate = (ts: number) => ts ? new Date(ts).toLocaleString(t('lang_code') || 'vi-VN', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';

onActivated(() => { gateway.sendMsg('get_tasks'); });
onMounted(() => { gateway.sendMsg('get_tasks'); });
</script>

<template>
  <div class="tm animate-fadeIn">
    <div class="page-header">
      <div class="hdr-top">
        <div>
          <h1 class="section-title">📝 {{ t('tm_title') }}</h1>
          <p class="page-desc">{{ t('tm_desc') }}</p>
        </div>
        <button class="btn btn-primary" @click="showForm = !showForm">
          {{ showForm ? t('tm_close') : t('tm_add') }}
        </button>
      </div>

      <!-- Stats Bar -->
      <div class="stats-bar">
        <button :class="['stat-chip', { active: _filter === 'all' }]" @click="_filter = 'all'">
          {{ t('tm_all') }} <span class="chip-n">{{ stats.total }}</span>
        </button>
        <button :class="['stat-chip pending', { active: _filter === 'pending' }]" @click="_filter = 'pending'">
          {{ t('tm_pending') }} <span class="chip-n">{{ stats.pending }}</span>
        </button>
        <button :class="['stat-chip progress', { active: _filter === 'in-progress' }]" @click="_filter = 'in-progress'">
          {{ t('tm_progress') }} <span class="chip-n">{{ stats.inProgress }}</span>
        </button>
        <button :class="['stat-chip done', { active: _filter === 'done' }]" @click="_filter = 'done'">
          {{ t('tm_done') }} <span class="chip-n">{{ stats.done }}</span>
        </button>
      </div>
    </div>

    <!-- Add Task Form -->
    <div v-if="showForm" class="card form-card animate-fadeIn">
      <div class="f-row">
        <div class="f-grow">
          <label class="form-label">{{ t('tm_task_name') }}</label>
          <input v-model="newTitle" class="input" :placeholder="t('tm_task_name_ph')" @keyup.enter="addTask" autofocus />
        </div>
        <div style="width:120px">
          <label class="form-label">{{ t('tm_priority') }}</label>
          <select v-model="newPriority" class="select">
            <option value="low">{{ t('tm_low') }}</option>
            <option value="medium">{{ t('tm_medium') }}</option>
            <option value="high">{{ t('tm_high') }}</option>
          </select>
        </div>
      </div>
      <div style="margin-top:8px">
        <label class="form-label">{{ t('tm_desc_label') }} <span class="form-hint">{{ t('tm_desc_hint') }}</span></label>
        <textarea v-model="newDesc" class="input textarea" :placeholder="t('tm_desc_ph')" rows="2"></textarea>
      </div>
      <div class="f-actions">
        <button class="btn btn-ghost" @click="showForm = false">{{ t('tm_cancel') }}</button>
        <button class="btn btn-primary" @click="addTask" :disabled="!newTitle.trim()">{{ t('tm_create') }}</button>
      </div>
    </div>

    <!-- Quick Actions -->
    <div class="quick-section">
      <span class="section-subtitle">{{ t('tm_quick') }}</span>
      <div class="quick-btns">
        <button class="q-btn" @click="quickAdd('Check email')">{{ t('tm_q_email') }}</button>
        <button class="q-btn" @click="quickAdd('News')">{{ t('tm_q_news') }}</button>
        <button class="q-btn" @click="quickAdd('Weather')">{{ t('tm_q_weather') }}</button>
        <button class="q-btn" @click="quickAdd('Plan tomorrow')">{{ t('tm_q_plan') }}</button>
        <button class="q-btn" @click="quickAdd('AI News')">{{ t('tm_q_ai') }}</button>
      </div>
    </div>

    <!-- Inline Planning Chat Panel -->
    <div v-if="activePlanId" class="plan-panel card animate-fadeIn">
      <div class="plan-header">
        <div class="plan-header-left">
          <span class="plan-icon">🤖</span>
          <div>
            <h3 class="plan-title">{{ t('tm_ai_plan_title') }}</h3>
            <p class="plan-subtitle">{{ tasks.find((t: any) => t.id === activePlanId)?.title || '' }}</p>
          </div>
        </div>
        <button class="btn btn-ghost btn-sm" @click="closePlanning" title="Đóng">✕</button>
      </div>
      
      <div class="plan-chat" ref="chatContainerRef">
        <div
          v-for="(msg, idx) in (planChats[activePlanId!] || [])"
          :key="idx"
          :class="['plan-msg', msg.role === 'ai' ? 'msg-ai' : 'msg-user']"
        >
          <div class="msg-avatar">{{ msg.role === 'ai' ? '🤖' : '👤' }}</div>
          <div class="msg-bubble">{{ msg.content }}</div>
        </div>
        
        <!-- Loading indicator -->
        <div v-if="planLoading" class="plan-msg msg-ai">
          <div class="msg-avatar">🤖</div>
          <div class="msg-bubble typing-indicator">
            <span></span><span></span><span></span>
          </div>
        </div>
      </div>
      
      <div class="plan-input-bar">
        <input
          v-model="planInput"
          class="input plan-input"
          :placeholder="t('tm_ai_plan_ph')"
          @keyup.enter="sendPlanMessage"
          :disabled="planLoading"
          autofocus
        />
        <button class="btn btn-primary plan-send" @click="sendPlanMessage" :disabled="!planInput.trim() || planLoading">
          ➤
        </button>
      </div>
    </div>

    <!-- Task List -->
    <div class="task-list">
      <div v-for="task in filteredTasks" :key="task.id" :class="['task-card', `st-${task.status}`]">
        <div class="tc-strip" :class="task.status"></div>
        <div class="tc-body">
          <div class="tc-top">
            <span class="tc-icon">{{ statusIcon(task.status) }}</span>
            <div class="tc-info">
              <h3 :class="['tc-title', { 'tc-done': task.status === 'done' }]">{{ task.title }}</h3>
              <p v-if="task.description" class="tc-desc">{{ task.description }}</p>
              <p v-if="task.result" class="tc-result">💡 {{ task.result }}</p>
            </div>
          </div>
          <div class="tc-footer">
            <div class="tc-meta">
              <span :class="['badge', priorityBadge(task.priority)]">{{ t(task.priority === 'high' ? 'tm_high' : task.priority === 'medium' ? 'tm_medium' : 'tm_low') }}</span>
              <span class="tc-date">{{ fmtDate(task.created_at) }}</span>
            </div>
            <div class="tc-actions">
              <button v-if="task.status !== 'done'" class="btn btn-accent btn-sm" @click="startPlanning(task)">{{ t('tm_btn_plan') }}</button>
              <button v-if="task.status === 'pending'" class="btn btn-primary btn-sm" @click="executeTask(task)">{{ t('tm_btn_exec') }}</button>
              <button v-if="task.status === 'in-progress'" class="btn btn-secondary btn-sm animate-pulse" @click="completeTask(task)">{{ t('tm_btn_done') }}</button>
              <button class="btn btn-ghost btn-sm" @click="deleteTask(task.id)">🗑️</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="filteredTasks.length === 0" class="empty-state">
        <div class="empty-icon">📋</div>
        <p v-if="_filter === 'all'">{{ t('tm_empty_all') }}</p>
        <p v-else>{{ t('tm_empty_filter', { status: _filter }) }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tm { padding: var(--space-lg); overflow-y: auto; height: 100%; }
.page-header { margin-bottom: var(--space-md); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }
.hdr-top { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--space-md); }

/* Stats */
.stats-bar { display: flex; gap: var(--space-xs); margin-top: var(--space-md); flex-wrap: wrap; }
.stat-chip {
  display: flex; align-items: center; gap: 6px;
  padding: 6px 14px; border-radius: 999px;
  background: var(--bg-secondary); border: 1px solid var(--border-default);
  color: var(--text-secondary); font-size: 12px; font-weight: 500;
  cursor: pointer; transition: all var(--transition-fast);
}
.stat-chip:hover { border-color: var(--text-muted); }
.stat-chip.active { border-color: var(--accent-start); color: var(--text-primary); background: var(--bg-tertiary); }
.stat-chip.pending.active { border-color: var(--color-info); }
.stat-chip.progress.active { border-color: var(--color-warning); }
.stat-chip.done.active { border-color: var(--color-success); }
.chip-n {
  background: var(--bg-tertiary); padding: 1px 7px; border-radius: 10px;
  font-size: 11px; font-weight: 700; min-width: 20px; text-align: center;
}

/* Form */
.form-card { margin-bottom: var(--space-md); }
.f-row { display: flex; gap: var(--space-sm); }
.f-grow { flex: 1; }
.f-actions { display: flex; justify-content: flex-end; gap: var(--space-sm); margin-top: var(--space-sm); }
.form-hint { color: var(--text-muted); font-size: 11px; font-weight: 400; }
.textarea { resize: vertical; min-height: 48px; font-family: inherit; line-height: 1.5; }

/* Quick Actions */
.quick-section { margin-bottom: var(--space-md); }
.quick-btns { display: flex; gap: var(--space-xs); flex-wrap: wrap; margin-top: var(--space-xs); }
.q-btn {
  padding: 6px 14px; border-radius: var(--radius-sm);
  background: var(--bg-secondary); border: 1px solid var(--border-default);
  color: var(--text-primary); font-size: 12px; cursor: pointer;
  transition: all var(--transition-fast);
}
.q-btn:hover { border-color: var(--accent-start); box-shadow: var(--shadow-glow); }

/* ═══════════════════════════════════════════ */
/* [v25] Inline Planning Chat Panel           */
/* ═══════════════════════════════════════════ */
.plan-panel {
  margin-bottom: var(--space-md);
  border: 1px solid rgba(124,58,237,0.3);
  background: var(--bg-secondary);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.plan-header {
  display: flex; justify-content: space-between; align-items: center;
  padding: 12px 16px;
  background: linear-gradient(135deg, rgba(124,58,237,0.08), rgba(99,102,241,0.08));
  border-bottom: 1px solid var(--border-default);
}

.plan-header-left { display: flex; gap: var(--space-sm); align-items: center; }
.plan-icon { font-size: 24px; }
.plan-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0; }
.plan-subtitle { font-size: 12px; color: var(--text-muted); margin: 2px 0 0; }

.plan-chat {
  max-height: 280px;
  overflow-y: auto;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.plan-msg {
  display: flex; gap: 8px; align-items: flex-start;
  max-width: 85%;
}

.plan-msg.msg-user { align-self: flex-end; flex-direction: row-reverse; }
.plan-msg.msg-ai { align-self: flex-start; }

.msg-avatar {
  width: 28px; height: 28px;
  border-radius: 50%;
  background: var(--bg-tertiary);
  display: flex; align-items: center; justify-content: center;
  font-size: 14px; flex-shrink: 0;
}

.msg-bubble {
  padding: 8px 12px;
  border-radius: 12px;
  font-size: 13px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.msg-ai .msg-bubble {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  border-bottom-left-radius: 4px;
}

.msg-user .msg-bubble {
  background: linear-gradient(135deg, var(--accent-start), var(--accent-end));
  color: #fff;
  border-bottom-right-radius: 4px;
}

/* Typing indicator */
.typing-indicator {
  display: flex; gap: 4px; align-items: center;
  padding: 10px 16px;
}
.typing-indicator span {
  width: 6px; height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  animation: typingBounce 1.4s infinite ease-in-out both;
}
.typing-indicator span:nth-child(1) { animation-delay: -0.32s; }
.typing-indicator span:nth-child(2) { animation-delay: -0.16s; }
.typing-indicator span:nth-child(3) { animation-delay: 0s; }

@keyframes typingBounce {
  0%, 80%, 100% { transform: scale(0); opacity: 0.4; }
  40% { transform: scale(1); opacity: 1; }
}

.plan-input-bar {
  display: flex; gap: 8px;
  padding: 10px 16px;
  border-top: 1px solid var(--border-default);
  background: var(--bg-primary);
}

.plan-input { flex: 1; }
.plan-send {
  padding: 8px 16px;
  font-size: 16px;
  min-width: 44px;
}

/* Task Cards */
.task-list { display: flex; flex-direction: column; gap: var(--space-sm); }
.task-card {
  display: flex; overflow: hidden;
  background: var(--bg-secondary); border: 1px solid var(--border-default);
  border-radius: var(--radius-md); transition: all var(--transition-fast);
}
.task-card:hover { border-color: rgba(124,58,237,.3); }
.task-card.st-done { opacity: 0.55; }

.tc-strip { width: 3px; flex-shrink: 0; }
.tc-strip.pending { background: var(--color-info); }
.tc-strip.in-progress { background: var(--color-warning); animation: pulse 1.5s infinite; }
.tc-strip.done { background: var(--color-success); }

.tc-body { flex: 1; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; }
.tc-top { display: flex; gap: var(--space-sm); align-items: flex-start; }
.tc-icon { font-size: 18px; margin-top: 1px; flex-shrink: 0; }
.tc-info { flex: 1; min-width: 0; }
.tc-title { font-size: 14px; font-weight: 600; color: var(--text-primary); }
.tc-title.tc-done { text-decoration: line-through; color: var(--text-muted); }
.tc-desc { font-size: 12px; color: var(--text-secondary); margin-top: 2px; white-space: pre-wrap; }
.tc-result { font-size: 12px; color: var(--color-success); margin-top: 4px; font-style: italic; }

.tc-footer { display: flex; justify-content: space-between; align-items: center; }
.tc-meta { display: flex; align-items: center; gap: var(--space-sm); }
.tc-date { font-size: 11px; color: var(--text-muted); }
.tc-actions { display: flex; gap: var(--space-xs); }
.btn-sm { padding: 5px 12px; font-size: 12px; }

.btn-accent {
  background: linear-gradient(135deg, rgba(124,58,237,0.15), rgba(99,102,241,0.15));
  border: 1px solid rgba(124,58,237,0.3);
  color: var(--accent-start);
  font-weight: 500;
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all var(--transition-fast);
}
.btn-accent:hover {
  background: linear-gradient(135deg, rgba(124,58,237,0.25), rgba(99,102,241,0.25));
  border-color: var(--accent-start);
}

/* Empty */
.empty-state {
  text-align: center; padding: var(--space-xl) var(--space-md);
  color: var(--text-muted); font-size: 14px;
}
.empty-icon { font-size: 40px; margin-bottom: var(--space-sm); opacity: 0.5; }
</style>
