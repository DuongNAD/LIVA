<script setup lang="ts">
defineProps<{
  currentLang: string;
  totalMemories: number;
  isConsolidating: boolean;
  isRefreshing: boolean;
  isRestarting: boolean;
  isRestartArmed: boolean;
  recentMemories: number;
  restartError: string;
}>();

defineEmits<{
  consolidate: [];
  refresh: [];
  restart: [];
}>();
</script>

<template>
  <div class="memory-header">
    <div class="page-header">
      <div class="header-row">
        <div>
          <h1 class="section-title">🧠 {{ currentLang === "vi-VN" ? "Không gian Trí nhớ" : "Memory Space" }}</h1>
          <p class="page-desc">
            {{ currentLang === "vi-VN"
              ? `Hệ thống ký ức hợp nhất — ${totalMemories} mục nhớ trên 5 tầng (L0 RAM, L0.5 Phiên, L1 Vector, L2 Sự kiện, L3 Sự thật).`
              : `Unified Hierarchical Memory — ${totalMemories} memories across 5 layers (L0 RAM, L0.5 Session, L1 Vectors, L2 Events, L3 Facts).` }}
          </p>
        </div>
        <div class="header-actions">
          <button class="btn btn-secondary btn-sm" :disabled="isConsolidating" @click="$emit('consolidate')">
            <span v-if="isConsolidating" class="spinner"></span>
            <span v-else>⚡ {{ currentLang === "vi-VN" ? "Kiểm tra projection" : "Validate projections" }}</span>
          </button>
          <button class="btn btn-secondary btn-sm" :disabled="isRefreshing" @click="$emit('refresh')">
            <span v-if="isRefreshing" class="spinner"></span>
            <span v-else>🔄 {{ currentLang === "vi-VN" ? "Làm mới" : "Refresh" }}</span>
          </button>
          <button
            class="btn btn-secondary restart-btn"
            :class="{ arming: isRestartArmed }"
            :disabled="isRestarting"
            @click="$emit('restart')"
          >
            <span v-if="isRestarting">⏳ {{ currentLang === "vi-VN" ? "Đang khởi động lại…" : "Restarting…" }}</span>
            <span v-else-if="isRestartArmed">⚠️ {{ currentLang === "vi-VN" ? "Bấm lần nữa để khởi động lại" : "Click again to restart" }}</span>
            <span v-else>♻️ {{ currentLang === "vi-VN" ? "Khởi động lại LIVA" : "Restart LIVA" }}</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="recentMemories > 0" class="vua-nho-banner">
      🧠
      <strong>{{ currentLang === "vi-VN" ? `LIVA vừa nhớ thêm ${recentMemories} điều` : `LIVA just remembered ${recentMemories} more` }}</strong>
      <span class="vua-nho-hint">
        {{ currentLang === "vi-VN"
          ? 'Bấm "Khởi động lại LIVA" rồi hỏi lại — ký ức nằm trong SQLite, không phải RAM.'
          : 'Hit “Restart LIVA” then ask again — memory lives in SQLite, not RAM.' }}
      </span>
    </div>
    <div v-if="restartError" class="vua-nho-banner loi">⚠️ {{ restartError }}</div>
  </div>
</template>

<style scoped>
.memory-header { display: flex; flex-direction: column; gap: 1rem; }
.page-header { border-bottom: 1px solid var(--border-default); padding-bottom: 1rem; }
.header-row { display: flex; justify-content: space-between; align-items: center; gap: 1rem; }
.header-actions { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; justify-content: flex-end; }
.section-title { font-size: 1.75rem; font-weight: 700; background: linear-gradient(135deg, #a855f7 0%, #3b82f6 100%); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; margin-bottom: 0.25rem; }
.page-desc { font-size: 0.875rem; color: var(--text-secondary); }
.spinner { display: inline-block; width: 1rem; height: 1rem; border: 2px solid rgba(255, 255, 255, 0.3); border-radius: 50%; border-top-color: #fff; animation: spin 1s ease-in-out infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.btn { display: inline-flex; align-items: center; justify-content: center; gap: 0.5rem; padding: 0.5rem 1rem; border-radius: var(--radius-md); font-weight: 600; font-size: 0.875rem; cursor: pointer; transition: all var(--transition-fast); border: 1px solid transparent; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-secondary { background: var(--bg-tertiary); color: var(--text-primary); border-color: var(--border-default); }
.btn-secondary:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--text-muted); }
.btn-sm { padding: 0.4rem 0.75rem; font-size: 0.75rem; }
.restart-btn.arming { border-color: #f59e0b; color: #f59e0b; background: rgba(245, 158, 11, 0.09); }
.vua-nho-banner { display: flex; align-items: center; gap: 0.65rem; padding: 0.75rem 1rem; border: 1px solid rgba(16, 185, 129, 0.35); border-radius: var(--radius-md); background: rgba(16, 185, 129, 0.08); color: var(--text-primary); }
.vua-nho-banner.loi { border-color: rgba(239, 68, 68, 0.4); background: rgba(239, 68, 68, 0.08); }
.vua-nho-hint { color: var(--text-secondary); font-size: 0.8rem; }
</style>
