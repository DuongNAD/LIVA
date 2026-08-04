<script setup lang="ts">
import type { MemoryTab } from "./memoryTypes";

defineProps<{ activeTab: MemoryTab; currentLang: string }>();
defineEmits<{ "update:activeTab": [value: MemoryTab] }>();
</script>

<template>
  <div class="tab-nav">
    <button class="tab-btn" :class="{ active: activeTab === 'l0' }" @click="$emit('update:activeTab', 'l0')">🧠 L0 RAM Cache</button>
    <button class="tab-btn" :class="{ active: activeTab === 'l0_5' }" @click="$emit('update:activeTab', 'l0_5')">📑 {{ currentLang === "vi-VN" ? "L0.5 Phiên" : "L0.5 Session" }}</button>
    <button class="tab-btn" :class="{ active: activeTab === 'vectors' }" @click="$emit('update:activeTab', 'vectors')">🌐 {{ currentLang === "vi-VN" ? "L1 Vector" : "L1 Vectors" }}</button>
    <button class="tab-btn" :class="{ active: activeTab === 'events' }" @click="$emit('update:activeTab', 'events')">🕰️ {{ currentLang === "vi-VN" ? "L2 Sự kiện" : "L2 Events" }}</button>
    <button class="tab-btn" :class="{ active: activeTab === 'facts' }" @click="$emit('update:activeTab', 'facts')">💾 {{ currentLang === "vi-VN" ? "L3 Sự thật" : "L3 Facts" }}</button>
  </div>
</template>

<style scoped>
.tab-nav { display: flex; gap: 0.5rem; border-bottom: 1px solid var(--border-default); overflow-x: auto; }
.tab-btn { padding: 0.75rem 1rem; border: none; border-bottom: 2px solid transparent; background: transparent; color: var(--text-secondary); cursor: pointer; white-space: nowrap; transition: color var(--transition-fast), border-color var(--transition-fast); }
.tab-btn:hover { color: var(--text-primary); }
.tab-btn.active { color: var(--accent-start); border-bottom-color: var(--accent-start); }
</style>
