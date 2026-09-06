<script setup lang="ts">
import { ref, computed } from 'vue';
import type { DiffHunk, HunkStatus } from '../types/diff';

const props = defineProps<{
  hunk: DiffHunk;
}>();

const emit = defineEmits<{
  (e: 'decision', status: HunkStatus): void;
}>();

const isEditing = ref(false);
const editContent = ref(props.hunk.diff_content);
const rejectReason = ref('');
const showRejectInput = ref(false);

const isPending = computed(() => props.hunk.status.type === 'pending');
const isApproved = computed(() => props.hunk.status.type === 'approved');
const isRejected = computed(() => props.hunk.status.type === 'rejected');
const isModified = computed(() => props.hunk.status.type === 'modified');

const approve = () => {
  emit('decision', { type: 'approved' });
};

const reject = () => {
  emit('decision', {
    type: 'rejected',
    payload: rejectReason.value.trim() ? { reason: rejectReason.value.trim() } : undefined,
  });
  showRejectInput.value = false;
};

const startEdit = () => {
  editContent.value = props.hunk.lines
    .filter((l) => l.line_type !== 'deletion')
    .map((l) => l.content)
    .join('\n');
  isEditing.value = true;
};

const saveEdit = () => {
  emit('decision', {
    type: 'modified',
    payload: { user_override: editContent.value },
  });
  isEditing.value = false;
};

const cancelEdit = () => {
  isEditing.value = false;
};

const resetDecision = () => {
  emit('decision', { type: 'pending' });
};
</script>

<template>
  <div
    class="diff-hunk-card"
    :class="[
      `status-${hunk.status.type}`,
      {
        'is-pending': isPending,
        'is-approved': isApproved,
        'is-rejected': isRejected,
        'is-modified': isModified,
      },
    ]"
  >
    <!-- Hunk Header -->
    <div class="hunk-header">
      <div class="hunk-meta">
        <span class="hunk-range">
          @@ -{{ hunk.old_start }},{{ hunk.old_lines }} +{{ hunk.new_start }},{{ hunk.new_lines }} @@
        </span>
        <span v-if="hunk.header" class="hunk-function-ctx">{{ hunk.header }}</span>
      </div>

      <!-- Status Indicator & Actions -->
      <div class="hunk-actions">
        <!-- Status Pill -->
        <span v-if="isPending" class="status-pill status-pill-pending">Pending</span>
        <span v-else-if="isApproved" class="status-pill status-pill-approved">Approved</span>
        <span v-else-if="isRejected" class="status-pill status-pill-rejected">Rejected</span>
        <span v-else-if="isModified" class="status-pill status-pill-modified">Modified</span>

        <!-- Action Buttons -->
        <template v-if="isPending && !isEditing">
          <button @click="approve" class="action-btn btn-approve-hunk" title="Approve this hunk">
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path stroke-width="2.5" d="M5 13l4 4L19 7" />
            </svg>
            Approve
          </button>
          <button
            @click="showRejectInput = !showRejectInput"
            class="action-btn btn-reject-hunk"
            title="Reject this hunk"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path stroke-width="2.5" d="M6 18L18 6M6 6l12 12" />
            </svg>
            Reject
          </button>
          <button @click="startEdit" class="action-btn btn-edit-hunk" title="Edit code before approving">
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path
                stroke-width="2"
                d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
              />
            </svg>
            Edit
          </button>
        </template>
        <template v-else-if="!isPending && !isEditing">
          <button @click="resetDecision" class="action-btn btn-reset-hunk" title="Undo decision">
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path
                stroke-width="2"
                d="M3 10h10a4 4 0 014 4v2m0 0l3-3m-3 3l-3-3M3 10l3-3M3 10l3 3"
              />
            </svg>
            Undo
          </button>
        </template>
      </div>
    </div>

    <!-- Reject Reason Sub-Panel -->
    <div v-if="showRejectInput" class="reject-reason-panel">
      <input
        v-model="rejectReason"
        type="text"
        placeholder="Reason for rejection (optional)..."
        class="reject-input"
        @keyup.enter="reject"
      />
      <div class="reject-btn-group">
        <button @click="reject" class="btn-xs btn-reject-confirm">Confirm Reject</button>
        <button @click="showRejectInput = false" class="btn-xs btn-cancel">Cancel</button>
      </div>
    </div>

    <!-- Hunk Code Body -->
    <div v-if="!isEditing" class="hunk-lines-container">
      <div
        v-for="(line, idx) in hunk.lines"
        :key="idx"
        class="diff-line"
        :class="`line-${line.line_type}`"
      >
        <div class="line-num line-num-old">{{ line.old_line_no ?? '' }}</div>
        <div class="line-num line-num-new">{{ line.new_line_no ?? '' }}</div>
        <div class="line-marker">
          {{ line.line_type === 'addition' ? '+' : line.line_type === 'deletion' ? '-' : ' ' }}
        </div>
        <div class="line-content">{{ line.content }}</div>
      </div>
    </div>

    <!-- Inline Code Editor Mode -->
    <div v-else class="hunk-editor-container">
      <textarea
        v-model="editContent"
        rows="8"
        class="hunk-textarea"
        placeholder="Edit hunk replacement code..."
        spellcheck="false"
      ></textarea>
      <div class="editor-actions">
        <button @click="saveEdit" class="btn btn-sm btn-approve">Save & Approve</button>
        <button @click="cancelEdit" class="btn btn-sm btn-cancel">Cancel</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.diff-hunk-card {
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  border-radius: 6px;
  overflow: hidden;
  background: #090b10;
  transition: border-color 0.15s ease;
}

.diff-hunk-card.is-pending {
  border-color: rgba(245, 158, 11, 0.3);
}

.diff-hunk-card.is-approved {
  border-color: rgba(16, 185, 129, 0.4);
}

.diff-hunk-card.is-rejected {
  border-color: rgba(239, 68, 68, 0.3);
  opacity: 0.65;
}

.diff-hunk-card.is-modified {
  border-color: rgba(168, 85, 247, 0.4);
}

.hunk-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 12px;
  background: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.hunk-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: monospace;
  font-size: 12px;
}

.hunk-range {
  color: #38bdf8;
  font-weight: 500;
}

.hunk-function-ctx {
  color: #94a3b8;
}

.hunk-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-pill {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
}

.status-pill-pending {
  background: rgba(245, 158, 11, 0.2);
  color: #fbbf24;
}

.status-pill-approved {
  background: rgba(16, 185, 129, 0.2);
  color: #34d399;
}

.status-pill-rejected {
  background: rgba(239, 68, 68, 0.2);
  color: #f87171;
}

.status-pill-modified {
  background: rgba(168, 85, 247, 0.2);
  color: #c084fc;
}

.action-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s ease;
}

.btn-approve-hunk {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
  border-color: rgba(16, 185, 129, 0.3);
}
.btn-approve-hunk:hover {
  background: rgba(16, 185, 129, 0.25);
}

.btn-reject-hunk {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
  border-color: rgba(239, 68, 68, 0.3);
}
.btn-reject-hunk:hover {
  background: rgba(239, 68, 68, 0.25);
}

.btn-edit-hunk {
  background: rgba(168, 85, 247, 0.15);
  color: #c084fc;
  border-color: rgba(168, 85, 247, 0.3);
}
.btn-edit-hunk:hover {
  background: rgba(168, 85, 247, 0.25);
}

.btn-reset-hunk {
  background: rgba(148, 163, 184, 0.15);
  color: #cbd5e1;
  border-color: rgba(148, 163, 184, 0.3);
}
.btn-reset-hunk:hover {
  background: rgba(148, 163, 184, 0.25);
}

.reject-reason-panel {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: rgba(239, 68, 68, 0.08);
  border-bottom: 1px solid rgba(239, 68, 68, 0.2);
}

.reject-input {
  flex: 1;
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 12px;
  color: #fecaca;
}

.reject-btn-group {
  display: flex;
  gap: 6px;
}

.btn-xs {
  font-size: 10px;
  padding: 3px 8px;
  border-radius: 4px;
  cursor: pointer;
  border: 1px solid transparent;
}

.btn-reject-confirm {
  background: #dc2626;
  color: #ffffff;
}

.btn-cancel {
  background: rgba(255, 255, 255, 0.1);
  color: #94a3b8;
}

.hunk-lines-container {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  overflow-x: auto;
}

.diff-line {
  display: flex;
  align-items: flex-start;
  min-height: 20px;
  padding: 0 4px;
}

.line-context {
  color: #cbd5e1;
}

.line-addition {
  background: rgba(16, 185, 129, 0.12);
  color: #a7f3d0;
}

.line-deletion {
  background: rgba(239, 68, 68, 0.12);
  color: #fca5a5;
}

.line-num {
  width: 36px;
  text-align: right;
  padding-right: 8px;
  color: #475569;
  user-select: none;
  flex-shrink: 0;
}

.line-marker {
  width: 16px;
  text-align: center;
  color: #64748b;
  user-select: none;
  flex-shrink: 0;
  font-weight: bold;
}

.line-addition .line-marker {
  color: #34d399;
}

.line-deletion .line-marker {
  color: #f87171;
}

.line-content {
  white-space: pre;
  flex: 1;
}

.hunk-editor-container {
  padding: 10px;
  background: #040508;
}

.hunk-textarea {
  width: 100%;
  background: #0d1117;
  color: #e6edf3;
  border: 1px solid rgba(168, 85, 247, 0.4);
  border-radius: 4px;
  font-family: monospace;
  font-size: 12px;
  padding: 8px;
  resize: vertical;
}

.editor-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 8px;
}

.btn-sm {
  padding: 4px 10px;
  font-size: 11px;
  border-radius: 4px;
  cursor: pointer;
  border: 1px solid transparent;
}

.btn-approve {
  background: rgba(16, 185, 129, 0.2);
  color: #34d399;
  border-color: rgba(16, 185, 129, 0.4);
}
</style>
