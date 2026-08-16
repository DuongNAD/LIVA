<script setup lang="ts">
/**
 * SkeletonLoader.vue — Standardized Shimmer Placeholder Component
 * ================================================================
 * Provides polished loading placeholders with customizable geometry,
 * line counts, animated gradient shimmer, and responsive layouts.
 */

export interface SkeletonProps {
  type?: 'text' | 'card' | 'avatar' | 'button' | 'list' | 'table' | 'circle' | 'rect';
  lines?: number;
  width?: string;
  height?: string;
  borderRadius?: string;
  count?: number;
  animated?: boolean;
  gap?: string;
}

const props = withDefaults(defineProps<SkeletonProps>(), {
  type: 'text',
  lines: 1,
  count: 1,
  animated: true,
  gap: '8px',
});

// Helper for dynamic inline styles of generic rect/circle
const getItemStyle = (index: number) => {
  const style: Record<string, string> = {};

  if (props.width) {
    // If lines > 1 and type is text, stagger last line width slightly for natural look
    if (props.type === 'text' && props.lines > 1 && index === props.lines - 1) {
      style.width = '70%';
    } else {
      style.width = props.width;
    }
  }

  if (props.height) {
    style.height = props.height;
  }

  if (props.borderRadius) {
    style.borderRadius = props.borderRadius;
  }

  return style;
};
</script>

<template>
  <div
    class="skeleton-root"
    :class="[`skeleton-type-${type}`, { 'skeleton-animated': animated }]"
    :style="{ gap }"
    role="status"
    aria-busy="true"
    aria-label="Loading..."
  >
    <!-- Text Skeleton (Repeated lines) -->
    <template v-if="type === 'text'">
      <div
        v-for="i in lines"
        :key="i"
        class="skeleton-line"
        :style="getItemStyle(i - 1)"
      />
    </template>

    <!-- Avatar Skeleton -->
    <template v-else-if="type === 'avatar'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-avatar"
        :style="getItemStyle(i - 1)"
      />
    </template>

    <!-- Card Skeleton -->
    <template v-else-if="type === 'card'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-card"
        :style="getItemStyle(i - 1)"
      >
        <div class="skeleton-card-header">
          <div class="skeleton-avatar" style="width: 36px; height: 36px;" />
          <div class="skeleton-card-meta">
            <div class="skeleton-line" style="width: 60%; height: 14px;" />
            <div class="skeleton-line" style="width: 40%; height: 10px;" />
          </div>
        </div>
        <div class="skeleton-card-body">
          <div class="skeleton-line" style="width: 100%; height: 12px;" />
          <div class="skeleton-line" style="width: 85%; height: 12px;" />
          <div class="skeleton-line" style="width: 50%; height: 12px;" />
        </div>
      </div>
    </template>

    <!-- List Skeleton -->
    <template v-else-if="type === 'list'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-list-item"
        :style="getItemStyle(i - 1)"
      >
        <div class="skeleton-avatar" style="width: 28px; height: 28px;" />
        <div class="skeleton-list-content">
          <div class="skeleton-line" style="width: 75%; height: 12px;" />
          <div class="skeleton-line" style="width: 45%; height: 10px;" />
        </div>
        <div class="skeleton-line" style="width: 50px; height: 16px; border-radius: 4px;" />
      </div>
    </template>

    <!-- Button Skeleton -->
    <template v-else-if="type === 'button'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-button"
        :style="getItemStyle(i - 1)"
      />
    </template>

    <!-- Table Skeleton -->
    <template v-else-if="type === 'table'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-table-row"
        :style="getItemStyle(i - 1)"
      >
        <div class="skeleton-line" style="width: 25%; height: 14px;" />
        <div class="skeleton-line" style="width: 35%; height: 14px;" />
        <div class="skeleton-line" style="width: 20%; height: 14px;" />
        <div class="skeleton-line" style="width: 15%; height: 14px;" />
      </div>
    </template>

    <!-- Generic Circle -->
    <template v-else-if="type === 'circle'">
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-circle"
        :style="getItemStyle(i - 1)"
      />
    </template>

    <!-- Generic Rect -->
    <template v-else>
      <div
        v-for="i in count"
        :key="i"
        class="skeleton-rect"
        :style="getItemStyle(i - 1)"
      />
    </template>
  </div>
</template>

<style scoped>
.skeleton-root {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.skeleton-type-avatar,
.skeleton-type-button,
.skeleton-type-circle {
  display: flex;
  flex-direction: row;
  align-items: center;
}

/* Base shimmer element */
.skeleton-line,
.skeleton-avatar,
.skeleton-card,
.skeleton-button,
.skeleton-circle,
.skeleton-rect,
.skeleton-list-item,
.skeleton-table-row {
  position: relative;
  overflow: hidden;
  background-color: var(--bg-tertiary, #141724);
  border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.03));
}

.skeleton-animated .skeleton-line::after,
.skeleton-animated .skeleton-avatar::after,
.skeleton-animated .skeleton-button::after,
.skeleton-animated .skeleton-circle::after,
.skeleton-animated .skeleton-rect::after,
.skeleton-animated .skeleton-card::after,
.skeleton-animated .skeleton-list-item::after,
.skeleton-animated .skeleton-table-row::after {
  content: '';
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  left: 0;
  transform: translateX(-100%);
  background: linear-gradient(
    90deg,
    transparent 0%,
    rgba(255, 255, 255, 0.06) 50%,
    transparent 100%
  );
  animation: shimmer 1.8s infinite cubic-bezier(0.4, 0, 0.2, 1);
}

[data-theme="light"] .skeleton-animated .skeleton-line::after,
[data-theme="light"] .skeleton-animated .skeleton-avatar::after,
[data-theme="light"] .skeleton-animated .skeleton-button::after,
[data-theme="light"] .skeleton-animated .skeleton-circle::after,
[data-theme="light"] .skeleton-animated .skeleton-rect::after,
[data-theme="light"] .skeleton-animated .skeleton-card::after,
[data-theme="light"] .skeleton-animated .skeleton-list-item::after,
[data-theme="light"] .skeleton-animated .skeleton-table-row::after {
  background: linear-gradient(
    90deg,
    transparent 0%,
    rgba(0, 0, 0, 0.04) 50%,
    transparent 100%
  );
}

@keyframes shimmer {
  100% {
    transform: translateX(100%);
  }
}

/* Individual shapes */
.skeleton-line {
  height: 14px;
  width: 100%;
  border-radius: var(--radius-sm, 6px);
}

.skeleton-avatar,
.skeleton-circle {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  flex-shrink: 0;
}

.skeleton-button {
  height: 36px;
  width: 100px;
  border-radius: var(--radius-sm, 6px);
}

.skeleton-rect {
  width: 100%;
  height: 60px;
  border-radius: var(--radius-md, 10px);
}

.skeleton-card {
  padding: 16px;
  border-radius: var(--radius-md, 12px);
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--bg-secondary, #0f111a);
}

.skeleton-card-header {
  display: flex;
  align-items: center;
  gap: 12px;
}

.skeleton-card-meta {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.skeleton-card-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.skeleton-list-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: var(--radius-sm, 8px);
  background: var(--bg-secondary, #0f111a);
}

.skeleton-list-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.skeleton-table-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 16px;
  border-radius: var(--radius-sm, 6px);
  background: var(--bg-secondary, #0f111a);
}
</style>
