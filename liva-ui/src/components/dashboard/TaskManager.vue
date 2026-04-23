<script setup lang="ts">
/**
 * TaskManager.vue — Work Task Management
 * ========================================
 * Add/manage tasks for LIVA to execute.
 */
import { ref } from "vue";

interface Task {
  id: number;
  title: string;
  description: string;
  status: 'pending' | 'in-progress' | 'done';
  priority: 'low' | 'medium' | 'high';
  createdAt: string;
}

const tasks = ref<Task[]>([
  { id: 1, title: 'Check email sáng', description: 'Kiểm tra email quan trọng', status: 'done', priority: 'high', createdAt: '2026-04-21 08:00' },
  { id: 2, title: 'Báo cáo tiến độ dự án', description: 'Tổng hợp và gửi báo cáo tuần', status: 'in-progress', priority: 'high', createdAt: '2026-04-21 10:00' },
  { id: 3, title: 'Search giá vé máy bay', description: 'Tìm vé rẻ Hà Nội - Đà Nẵng', status: 'pending', priority: 'low', createdAt: '2026-04-21 14:00' },
]);

const newTaskTitle = ref('');
const newTaskDesc = ref('');
const newTaskPriority = ref<'low' | 'medium' | 'high'>('medium');

let nextId = 4;

const addTask = () => {
  if (!newTaskTitle.value.trim()) return;
  tasks.value.unshift({
    id: nextId++,
    title: newTaskTitle.value.trim(),
    description: newTaskDesc.value.trim(),
    status: 'pending',
    priority: newTaskPriority.value,
    createdAt: new Date().toLocaleString('vi-VN'),
  });
  newTaskTitle.value = '';
  newTaskDesc.value = '';
};

const executeTask = (task: Task) => {
  task.status = 'in-progress';
  // TODO: gửi qua WebSocket để LIVA thực hiện
  console.log(`[TaskManager] Executing: ${task.title}`);
};

const completeTask = (task: Task) => {
  task.status = 'done';
};

const deleteTask = (id: number) => {
  tasks.value = tasks.value.filter(t => t.id !== id);
};

const statusIcon = (s: string) => s === 'done' ? '✅' : s === 'in-progress' ? '⏳' : '⏹️';
const priorityColor = (p: string) => p === 'high' ? 'danger' : p === 'medium' ? 'warning' : 'info';
</script>

<template>
  <div class="task-manager animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">📝 Task Manager</h1>
      <p class="page-desc">Giao việc cho LIVA và theo dõi tiến độ</p>
    </div>

    <!-- Add Task Form -->
    <div class="card add-task-form">
      <div class="form-row" style="gap: 8px; align-items: flex-end;">
        <div class="form-group" style="flex: 1; margin-bottom: 0;">
          <label class="form-label">Tên công việc</label>
          <input v-model="newTaskTitle" class="input" placeholder="VD: Tìm giá khách sạn Đà Lạt..." @keyup.enter="addTask" />
        </div>
        <select v-model="newTaskPriority" class="select" style="width: 130px;">
          <option value="low">🟢 Thấp</option>
          <option value="medium">🟡 Trung bình</option>
          <option value="high">🔴 Cao</option>
        </select>
        <button class="btn btn-primary" @click="addTask">➕ Thêm</button>
      </div>
      <div class="form-group" style="margin-top: 8px; margin-bottom: 0;">
        <input v-model="newTaskDesc" class="input" placeholder="Mô tả chi tiết (tùy chọn)..." />
      </div>
    </div>

    <!-- Quick Actions -->
    <div class="quick-actions">
      <span class="section-subtitle">Quick Actions</span>
      <div class="quick-btns">
        <button class="btn btn-secondary" @click="newTaskTitle = 'Check email quan trọng'; addTask()">📧 Check Email</button>
        <button class="btn btn-secondary" @click="newTaskTitle = 'Tóm tắt tin tức hôm nay'; addTask()">📰 Tin tức</button>
        <button class="btn btn-secondary" @click="newTaskTitle = 'Kiểm tra thời tiết'; addTask()">🌤️ Thời tiết</button>
        <button class="btn btn-secondary" @click="newTaskTitle = 'Lên kế hoạch ngày mai'; addTask()">📋 Lên kế hoạch</button>
      </div>
    </div>

    <!-- Task List -->
    <div class="task-list">
      <div v-for="task in tasks" :key="task.id" :class="['card task-item', `status-${task.status}`]">
        <div class="task-main">
          <span class="task-status">{{ statusIcon(task.status) }}</span>
          <div class="task-content">
            <h3 :class="['task-title', { 'task-done': task.status === 'done' }]">{{ task.title }}</h3>
            <p v-if="task.description" class="task-desc">{{ task.description }}</p>
            <div class="task-meta">
              <span :class="['badge', `badge-${priorityColor(task.priority)}`]">{{ task.priority }}</span>
              <span class="task-date">{{ task.createdAt }}</span>
            </div>
          </div>
        </div>
        <div class="task-actions">
          <button v-if="task.status === 'pending'" class="btn btn-primary btn-sm" @click="executeTask(task)">▶ Thực hiện</button>
          <button v-if="task.status === 'in-progress'" class="btn btn-secondary btn-sm" @click="completeTask(task)">✓ Xong</button>
          <button class="btn btn-ghost btn-sm" @click="deleteTask(task.id)">🗑️</button>
        </div>
      </div>

      <div v-if="tasks.length === 0" class="empty-state">
        <p>📋 Chưa có công việc nào. Hãy thêm task mới!</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.task-manager { padding: var(--space-lg); overflow-y: auto; height: 100%; }
.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.add-task-form { margin-bottom: var(--space-md); }

.quick-actions { margin-bottom: var(--space-lg); }
.quick-btns { display: flex; gap: var(--space-sm); flex-wrap: wrap; margin-top: var(--space-sm); }

.task-list { display: flex; flex-direction: column; gap: var(--space-sm); }

.task-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-md);
  transition: all var(--transition-fast);
}

.task-item.status-done { opacity: 0.6; }

.task-main { display: flex; align-items: flex-start; gap: var(--space-md); flex: 1; }
.task-status { font-size: 18px; margin-top: 2px; }
.task-content { flex: 1; }
.task-title { font-size: 14px; font-weight: 600; color: var(--text-primary); }
.task-title.task-done { text-decoration: line-through; color: var(--text-muted); }
.task-desc { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
.task-meta { display: flex; align-items: center; gap: var(--space-sm); margin-top: 6px; }
.task-date { font-size: 11px; color: var(--text-muted); }

.task-actions { display: flex; gap: var(--space-xs); flex-shrink: 0; }
.btn-sm { padding: 6px 12px; font-size: 12px; }

.empty-state { text-align: center; padding: var(--space-xl); color: var(--text-muted); font-size: 14px; }
</style>
