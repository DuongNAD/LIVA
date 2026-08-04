<script setup lang="ts">
import type { MemoryTab } from "./memoryTypes";

defineProps<{
  activeTab: MemoryTab;
  currentLang: string;
  l0Count: number;
  l05Size: string;
  l05NotWired: boolean;
  factsCount: number;
  eventsCount: number;
  vectorsCount: number;
}>();

defineEmits<{ "update:activeTab": [value: MemoryTab] }>();
</script>

<template>
  <div class="stats-grid five-cols">
    <div class="stat-card l0-stat" :class="{ active: activeTab === 'l0' }" @click="$emit('update:activeTab', 'l0')">
      <div class="stat-icon">🧠</div><div class="stat-info"><h3>{{ l0Count }}</h3><p>{{ currentLang === "vi-VN" ? "Trí nhớ RAM L0" : "L0 RAM Cache" }}</p></div>
    </div>
    <div class="stat-card l0-5-stat" :class="{ active: activeTab === 'l0_5', 'chua-noi-day': l05NotWired }" @click="$emit('update:activeTab', 'l0_5')">
      <div class="stat-icon">📑</div><div class="stat-info"><h3>{{ l05Size }}</h3><p>{{ currentLang === "vi-VN" ? "Phiên L0.5" : "L0.5 Session" }} <span v-if="l05NotWired" class="chua-co-badge">{{ currentLang === "vi-VN" ? "chưa có" : "not wired" }}</span></p></div>
    </div>
    <div class="stat-card facts-stat" :class="{ active: activeTab === 'facts' }" @click="$emit('update:activeTab', 'facts')">
      <div class="stat-icon">💾</div><div class="stat-info"><h3>{{ factsCount }}</h3><p>{{ currentLang === "vi-VN" ? "Sự thật L3" : "L3 Facts" }}</p></div>
    </div>
    <div class="stat-card events-stat" :class="{ active: activeTab === 'events' }" @click="$emit('update:activeTab', 'events')">
      <div class="stat-icon">⚡</div><div class="stat-info"><h3>{{ eventsCount }}</h3><p>{{ currentLang === "vi-VN" ? "Sự kiện L2" : "L2 Events" }}</p></div>
    </div>
    <div class="stat-card vectors-stat" :class="{ active: activeTab === 'vectors' }" @click="$emit('update:activeTab', 'vectors')">
      <div class="stat-icon">🌐</div><div class="stat-info"><h3>{{ vectorsCount }}</h3><p>{{ currentLang === "vi-VN" ? "Vector L1" : "L1 Vectors" }}</p></div>
    </div>
  </div>
</template>

<style scoped>
.stats-grid { display: grid; gap: 0.85rem; }
.stats-grid.five-cols { grid-template-columns: repeat(5, 1fr); }
.stat-card { display: flex; align-items: center; gap: 0.75rem; padding: 0.85rem 1rem; background: var(--bg-secondary); border: 1px solid var(--border-default); border-radius: var(--radius-md); cursor: pointer; transition: all var(--transition-normal); box-shadow: var(--shadow-card); }
.stat-card:hover { background: var(--bg-hover); border-color: var(--text-muted); transform: translateY(-2px); box-shadow: var(--shadow-md); }
.stat-card.active.l0-stat { border-color: #3b82f6; background: rgba(59, 130, 246, 0.08); }
.stat-card.active.l0-5-stat { border-color: #10b981; background: rgba(16, 185, 129, 0.08); }
.stat-card.active.facts-stat { border-color: #a855f7; background: rgba(168, 85, 247, 0.08); }
.stat-card.active.events-stat { border-color: #ec4899; background: rgba(236, 72, 153, 0.08); }
.stat-card.active.vectors-stat { border-color: #f59e0b; background: rgba(245, 158, 11, 0.08); }
.stat-card.chua-noi-day { opacity: 0.62; border-style: dashed; }
.stat-icon { font-size: 1.5rem; }
.stat-info h3 { font-size: 1.25rem; font-weight: 700; margin: 0; }
.stat-info p { font-size: 0.7rem; color: var(--text-muted); margin: 0.1rem 0 0; }
.chua-co-badge { margin-left: 0.2rem; padding: 0.05rem 0.3rem; border-radius: 999px; background: var(--bg-tertiary); font-size: 0.58rem; }
@media (max-width: 900px) { .stats-grid.five-cols { grid-template-columns: repeat(3, 1fr); } }
@media (max-width: 600px) { .stats-grid.five-cols { grid-template-columns: repeat(2, 1fr); } }
</style>
