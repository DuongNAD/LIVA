<script setup lang="ts">
import { ref, computed } from 'vue';
import { useLivingCanvas } from '../composables/useLivingCanvas';
import DiffHunkCard from './DiffHunkCard.vue';
import type { HunkStatus } from '../types/diff';

const {
  activeSession,
  submitHunkDecision,
  approveAllPending,
  rejectAllPending,
  submitSessionDecisions,
  isSubmitting,
} = useLivingCanvas();

const searchQuery = ref('');
const statusFilter = ref<'all' | 'pending' | 'decided'>('all');
const collapsedFiles = ref<Record<string, boolean>>({});

const toggleFileCollapse = (filePath: string) => {
  collapsedFiles.value[filePath] = !collapsedFiles.value[filePath];
};

const filteredFiles = computed(() => {
  if (!activeSession.value) return [];
  return activeSession.value.files.filter((file) => {
    const path = file.new_path || file.old_path || '';
    const matchesSearch = path.toLowerCase().includes(searchQuery.value.toLowerCase());
    if (!matchesSearch) return false;

    if (statusFilter.value === 'pending') {
      return file.hunks.some((h) => h.status.type === 'pending');
    } else if (statusFilter.value === 'decided') {
      return file.hunks.some((h) => h.status.type !== 'pending');
    }
    return true;
  });
});

const stats = computed(() => {
  if (!activeSession.value) return { total: 0, pending: 0, approved: 0, rejected: 0, modified: 0 };
  let total = 0,
    pending = 0,
    approved = 0,
    rejected = 0,
    modified = 0;
  for (const file of activeSession.value.files) {
    for (const hunk of file.hunks) {
      total++;
      if (hunk.status.type === 'pending') pending++;
      else if (hunk.status.type === 'approved') approved++;
      else if (hunk.status.type === 'rejected') rejected++;
      else if (hunk.status.type === 'modified') modified++;
    }
  }
  return { total, pending, approved, rejected, modified };
});

const onDecision = async (hunkId: string, status: HunkStatus) => {
  if (!activeSession.value) return;
  await submitHunkDecision(activeSession.value.session_id, hunkId, status);
};
</script>

<template>
  <div class="diff-reviewer">
    <!-- Top Action & Summary Bar -->
    <div class="reviewer-toolbar">
      <div class="stats-summary">
        <span class="stat-item font-semibold text-slate-200">
          {{ stats.total }} Hunk{{ stats.total !== 1 ? 's' : '' }}
        </span>
        <span class="stat-badge bg-amber-500/15 text-amber-300 border border-amber-500/30">
          {{ stats.pending }} Pending
        </span>
        <span class="stat-badge bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
          {{ stats.approved }} Approved
        </span>
        <span class="stat-badge bg-rose-500/15 text-rose-300 border border-rose-500/30">
          {{ stats.rejected }} Rejected
        </span>
        <span
          v-if="stats.modified > 0"
          class="stat-badge bg-purple-500/15 text-purple-300 border border-purple-500/30"
        >
          {{ stats.modified }} Modified
        </span>
      </div>

      <div class="batch-actions">
        <button
          @click="approveAllPending"
          :disabled="stats.pending === 0 || isSubmitting"
          class="btn btn-sm btn-approve"
        >
          Approve All
        </button>
        <button
          @click="rejectAllPending"
          :disabled="stats.pending === 0 || isSubmitting"
          class="btn btn-sm btn-reject"
        >
          Reject All
        </button>
        <button
          @click="submitSessionDecisions"
          :disabled="stats.pending > 0 || isSubmitting"
          class="btn btn-sm btn-primary"
        >
          Submit & Resume
        </button>
      </div>
    </div>

    <!-- Search & Filter Bar -->
    <div class="filter-bar">
      <input
        v-model="searchQuery"
        type="text"
        placeholder="Filter files (e.g. src/agent/)..."
        class="search-input"
      />
      <div class="filter-pills">
        <button :class="['pill', { active: statusFilter === 'all' }]" @click="statusFilter = 'all'">
          All
        </button>
        <button
          :class="['pill', { active: statusFilter === 'pending' }]"
          @click="statusFilter = 'pending'"
        >
          Pending
        </button>
        <button
          :class="['pill', { active: statusFilter === 'decided' }]"
          @click="statusFilter = 'decided'"
        >
          Decided
        </button>
      </div>
    </div>

    <!-- Files Diff Container -->
    <div class="diff-files-list">
      <div v-if="!activeSession || activeSession.files.length === 0" class="empty-state">
        <svg
          class="w-12 h-12 text-slate-600 mb-3"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
        <p class="text-slate-400 font-medium">No Pending Diff Review</p>
        <p class="text-slate-600 text-sm">
          When an agent proposes code modifications, hunks will appear here for review.
        </p>
      </div>

      <div
        v-for="file in filteredFiles"
        :key="file.new_path || file.old_path || 'unknown'"
        class="file-diff-card"
      >
        <!-- File Header -->
        <div
          class="file-header"
          @click="toggleFileCollapse(file.new_path || file.old_path || '')"
        >
          <div class="file-info">
            <svg
              class="w-4 h-4 transition-transform duration-200"
              :class="{ 'rotate-90': !collapsedFiles[file.new_path || file.old_path || ''] }"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
            >
              <path stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
            <span class="file-path">{{ file.new_path || file.old_path }}</span>
            <span v-if="file.is_new" class="badge-tag bg-emerald-500/20 text-emerald-400">NEW</span>
            <span v-else-if="file.is_deleted" class="badge-tag bg-rose-500/20 text-rose-400"
              >DELETED</span
            >
            <span v-else-if="file.is_renamed" class="badge-tag bg-amber-500/20 text-amber-400"
              >RENAMED</span
            >
          </div>

          <div class="file-summary">
            <span class="text-xs text-slate-400">
              {{ file.hunks.length }} Hunk{{ file.hunks.length > 1 ? 's' : '' }}
            </span>
          </div>
        </div>

        <!-- Hunks List -->
        <div v-show="!collapsedFiles[file.new_path || file.old_path || '']" class="file-hunks">
          <DiffHunkCard
            v-for="hunk in file.hunks"
            :key="hunk.hunk_id"
            :hunk="hunk"
            @decision="(status) => onDecision(hunk.hunk_id, status)"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.diff-reviewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-secondary, #0f111a);
  overflow: hidden;
}

.reviewer-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  background: var(--bg-tertiary, #141724);
  border-bottom: 1px solid var(--border-default, rgba(255, 255, 255, 0.06));
}

.stats-summary {
  display: flex;
  align-items: center;
  gap: 8px;
}

.stat-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 4px;
  font-weight: 500;
}

.batch-actions {
  display: flex;
  gap: 8px;
}

.btn {
  display: inline-flex;
  align-items: center;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s ease;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-approve {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
  border-color: rgba(16, 185, 129, 0.3);
}
.btn-approve:hover:not(:disabled) {
  background: rgba(16, 185, 129, 0.25);
}

.btn-reject {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
  border-color: rgba(239, 68, 68, 0.3);
}
.btn-reject:hover:not(:disabled) {
  background: rgba(239, 68, 68, 0.25);
}

.btn-primary {
  background: linear-gradient(135deg, #a855f7, #6366f1);
  color: #ffffff;
}

.filter-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 16px;
  gap: 12px;
  border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.04));
}

.search-input {
  flex: 1;
  background: rgba(0, 0, 0, 0.25);
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  border-radius: 6px;
  padding: 6px 12px;
  font-size: 13px;
  color: #f1f5f9;
}

.filter-pills {
  display: flex;
  gap: 4px;
}

.pill {
  padding: 4px 10px;
  font-size: 12px;
  border-radius: 4px;
  background: transparent;
  color: #94a3b8;
  border: 1px solid transparent;
  cursor: pointer;
}

.pill.active {
  background: rgba(168, 85, 247, 0.2);
  color: #c084fc;
  border-color: rgba(168, 85, 247, 0.4);
}

.diff-files-list {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.file-diff-card {
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  overflow: hidden;
  background: #111420;
}

.file-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  cursor: pointer;
  user-select: none;
}

.file-info {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: monospace;
  font-size: 13px;
  color: #e2e8f0;
}

.badge-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
}

.file-hunks {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--bg-secondary, #0f111a);
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 260px;
  text-align: center;
}
</style>
