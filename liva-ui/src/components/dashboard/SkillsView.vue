<script setup lang="ts">
/**
 * SkillsView.vue — Skills Overview Grid
 * ========================================
 * Displays all 29 LIVA skills in categorized card layout.
 */
import { computed } from "vue";
import { useGateway } from "../../composables/useGateway";

interface Skill {
  name: string;
  icon: string;
  description: string;
  category: string;
}

const gateway = useGateway();

const skills = computed<Skill[]>(() => {
  if (!gateway.isConnected.value) return [];
  if (gateway.skillsList.value && gateway.skillsList.value.length > 0) {
    return gateway.skillsList.value.map((s: any) => ({
      name: s.name,
      icon: s.icon || '⚡', 
      description: s.description || 'No description',
      category: s.isCoreSkill ? 'Core' : 'Extension'
    }));
  }
  return [];
});

// Group by category
const categories = computed(() => [...new Set(skills.value.map(s => s.category))]);
const skillsByCategory = (cat: string) => skills.value.filter(s => s.category === cat);
</script>

<template>
  <div class="skills-view animate-fadeIn">
    <div class="page-header">
      <h1 class="section-title">⚡ Skills Overview</h1>
      <p class="page-desc">Tổng quan {{ skills.length }} kỹ năng của LIVA</p>
    </div>

    <div v-for="category in categories" :key="category" class="skill-category">
      <h2 class="category-title">{{ category }}</h2>
      <div class="skill-grid">
        <div v-for="skill in skillsByCategory(category)" :key="skill.name" class="card skill-card">
          <div class="skill-icon">{{ skill.icon }}</div>
          <div class="skill-info">
            <h3 class="skill-name">{{ skill.name }}</h3>
            <p class="skill-desc">{{ skill.description }}</p>
          </div>
          <span class="badge badge-success">Active</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.skills-view { padding: var(--space-lg); overflow-y: auto; height: 100%; }
.page-header { margin-bottom: var(--space-lg); }
.page-desc { color: var(--text-secondary); font-size: 13px; margin-top: 4px; }

.skill-category { margin-bottom: var(--space-xl); }
.category-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: var(--space-md);
  padding-bottom: var(--space-sm);
  border-bottom: 1px solid var(--border-default);
}

.skill-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--space-sm);
}

.skill-card {
  display: flex;
  align-items: center;
  gap: var(--space-md);
  padding: 12px 16px;
}

.skill-icon { font-size: 24px; flex-shrink: 0; }
.skill-info { flex: 1; min-width: 0; }
.skill-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.skill-desc { font-size: 11px; color: var(--text-secondary); margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
