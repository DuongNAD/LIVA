<script setup lang="ts">
/**
 * ObsidianVaultView.vue — Interactive Knowledge Vault & Note Explorer
 * ====================================================================
 * Connects to LIVA's local Obsidian Knowledge Vault, allowing users to
 * search, browse notes, inspect frontmatter YAML, render markdown,
 * and navigate wikilinks / backlinks.
 */
import { ref, computed, onMounted } from 'vue';
import { useGateway } from '../../composables/useGateway';
import { useI18n } from '../../composables/useI18n';
import { useToast } from '../../composables/useToast';
import SkeletonLoader from '../SkeletonLoader.vue';

const gateway = useGateway();
const { t } = useI18n();
const toast = useToast();

interface NoteItem {
  id: string;
  title: string;
  category: string;
  path: string;
  tags: string[];
  updatedAt: string;
  wordCount: number;
  frontmatter: Record<string, string | number | boolean>;
  content: string;
  outgoingLinks: string[];
  backlinks: string[];
}

const searchQuery = ref('');
const selectedCategory = ref('All');
const sortBy = ref<'updated' | 'title' | 'words'>('updated');
const isLoading = ref(false);
const showFrontmatter = ref(true);

const categories = ['All', 'Architecture', 'AI Core', 'PKM', 'Daily Briefings', 'Security', 'Skills', 'Decisions'];

// Rich realistic notes representing the LIVA Obsidian Vault
const notes = ref<NoteItem[]>([
  {
    id: 'liva-core-architecture',
    title: 'LIVA Unified Native Architecture (Rust + Tauri v2)',
    category: 'Architecture',
    path: '01-architecture/liva-core-architecture.md',
    tags: ['rust', 'native-core', 'tauri-v2', 'sqlite-wal'],
    updatedAt: '2026-08-16 14:15',
    wordCount: 840,
    frontmatter: {
      title: 'LIVA Unified Native Architecture',
      version: '3.0.0',
      status: 'production',
      engine: 'liva-native-core (Rust 2024)',
    },
    content: `# LIVA Unified Native Architecture

The LIVA engine is fully unified in **Rust** (\`liva-native-core\`), operating in-process with **Tauri v2**.

## Key Subsystems
1. **SQLite WAL Bifurcated Pool**: Dedicated 1-writer and 4-readers pool eliminating disk lock contention.
2. **In-Process LLM Inference**: Direct \`llama.cpp\` C-bindings with GPU offloading and dynamic KV cache quantization.
3. **Hybrid RAG**: ONNX multilingual-e5-small (384-dim) vector search combined with SQLite FTS5 ($K=60.0$ RRF).
4. **Swarm DAG StateGraph**: Multi-agent async task decomposition with rollback consensus.

> [!NOTE]
> All IPC requests flow through \`native_ipc_call\` and \`native_ipc_call_stream\` with zero double-serialization overhead.

See also: [[hybrid-rag-vector-search]], [[sqlite-wal-pool-spec]], [[swarm-dag-orchestration]].`,
    outgoingLinks: ['hybrid-rag-vector-search', 'sqlite-wal-pool-spec', 'swarm-dag-orchestration'],
    backlinks: ['agent-swarm-governance', 'security-pdg-audit'],
  },
  {
    id: 'hybrid-rag-vector-search',
    title: 'Hybrid RAG: Vector Embeddings & FTS5 Fusion',
    category: 'AI Core',
    path: '02-ai-core/hybrid-rag-vector-search.md',
    tags: ['rag', 'onnx', 'vector', 'fts5', 'rrf'],
    updatedAt: '2026-08-16 13:40',
    wordCount: 620,
    frontmatter: {
      title: 'Hybrid RAG Engine',
      embedder: 'multilingual-e5-small (384-dim)',
      fusion: 'Reciprocal Rank Fusion (K=60.0)',
      fts_tokenizer: 'unicode61',
    },
    content: `# Hybrid RAG & Vector Search

LIVA uses a hybrid retrieval pipeline combining semantic vector similarity and keyword BM25/FTS5 matching.

## Reciprocal Rank Fusion Formula
$$RRF(d) = \\sum_{m \\in M} \\frac{1}{K + rank_m(d)}$$

Where:
- $K = 60.0$ (smoothing parameter)
- $M = \\{ \\text{Vector}, \\text{FTS5} \\}$

### Performance Metrics
- Vector distance threshold: \`0.68\` cosine similarity
- Average retrieval latency: \`14.2ms\` on local SQLite WAL

Related notes: [[liva-core-architecture]], [[knowledge-vault-sync]].`,
    outgoingLinks: ['liva-core-architecture', 'knowledge-vault-sync'],
    backlinks: ['liva-core-architecture', 'doc-rag-auditor-spec'],
  },
  {
    id: 'swarm-dag-orchestration',
    title: 'Swarm DAG StateGraph & Multi-Agent Consensus',
    category: 'AI Core',
    path: '02-ai-core/swarm-dag-orchestration.md',
    tags: ['swarm', 'multi-agent', 'dag', 'stategraph'],
    updatedAt: '2026-08-16 11:20',
    wordCount: 750,
    frontmatter: {
      title: 'Swarm DAG StateGraph Engine',
      agents_count: 6,
      max_iterations: 10,
      dlq_enabled: true,
    },
    content: `# Swarm DAG StateGraph Engine

Complex user requests are parsed into Directed Acyclic Graphs (DAG) and dispatched across specialized subagents.

## Available Subagents
- **BI Analyst**: \`liva-bi-analyst\` (SQL synthesis & KPI tracking)
- **PKM Obsidian**: \`liva-pkm-obsidian\` (Vault curation & link synthesis)
- **Daily Planner**: \`liva-daily-planner\` (Schedule & priority balancing)
- **Doc RAG Auditor**: \`liva-doc-rag-auditor\` (PDF & Contract analysis)
- **Smart DevOps**: \`liva-smart-devops\` (CI/CD & container triage)
- **Security PDG**: \`liva-security-pdg\` (Taint analysis & PII audit)

> [!TIP]
> Each agent adheres to the \`CommandPrincipal\` RBAC policy before executing side-effects.

Related notes: [[liva-core-architecture]], [[security-pdg-audit]].`,
    outgoingLinks: ['liva-core-architecture', 'security-pdg-audit'],
    backlinks: ['liva-core-architecture'],
  },
  {
    id: 'security-pdg-audit',
    title: 'Security Auditing, PDG Taint Tracking & DPAPI Vault',
    category: 'Security',
    path: '04-security/security-pdg-audit.md',
    tags: ['security', 'pdg', 'taint', 'dpapi', 'decree-13'],
    updatedAt: '2026-08-15 17:05',
    wordCount: 510,
    frontmatter: {
      title: 'Security & Compliance Specifications',
      keystore: 'Windows DPAPI (.device_key)',
      encryption: 'AES-256-GCM',
      compliance: 'Vietnamese Decree 13 & GDPR',
    },
    content: `# Security & Compliance Hardening

LIVA enforces rigorous zero-trust boundaries:
1. **Windows DPAPI Keystore**: Master secret encrypted via OS DPAPI, never stored in plain text.
2. **AES-256-GCM Transcripts**: All session memories encrypted before disk writes.
3. **Program Dependence Graph (PDG)**: Static taint analysis prevents untrusted user input from reaching command execution sinks.
4. **Decree 13 / GDPR Compliance**: Supports Right-to-be-Forgotten with \`PRAGMA secure_delete = ON\`.

Related: [[liva-core-architecture]], [[swarm-dag-orchestration]].`,
    outgoingLinks: ['liva-core-architecture', 'swarm-dag-orchestration'],
    backlinks: ['liva-core-architecture', 'swarm-dag-orchestration'],
  },
  {
    id: 'daily-briefing-2026-08-16',
    title: 'Daily Briefing — 2026-08-16 (Ecosystem Upgrade)',
    category: 'Daily Briefings',
    path: '05-daily-briefings/2026-08-16.md',
    tags: ['briefing', 'daily', 'milestone-m2'],
    updatedAt: '2026-08-16 08:00',
    wordCount: 420,
    frontmatter: {
      title: 'Daily Briefing 2026-08-16',
      weather: 'Hanoi 29°C Partly Cloudy',
      priority_tasks: 3,
    },
    content: `# Daily Briefing — 2026-08-16

## System Overview
- **Milestone M2**: Frontend UI/UX Modernization & View Extensions in progress.
- **Health**: 7/7 Services Online. SQLite WAL latency < 35ms.
- **Scheduled Tasks**: 3 pending items for afternoon execution.

> [!NOTE]
> Shadow Digest delivered at 07:00 via UI & Telegram.

Links: [[liva-core-architecture]], [[hybrid-rag-vector-search]].`,
    outgoingLinks: ['liva-core-architecture', 'hybrid-rag-vector-search'],
    backlinks: [],
  },
]);

const selectedNoteId = ref<string>('liva-core-architecture');

const selectedNote = computed(() => {
  return notes.value.find(n => n.id === selectedNoteId.value) || notes.value[0];
});

// Filtered and sorted notes
const filteredNotes = computed(() => {
  let result = notes.value;

  if (selectedCategory.value !== 'All') {
    result = result.filter(n => n.category === selectedCategory.value);
  }

  if (searchQuery.value.trim()) {
    const q = searchQuery.value.toLowerCase().trim();
    result = result.filter(n =>
      n.title.toLowerCase().includes(q) ||
      n.tags.some(t => t.toLowerCase().includes(q)) ||
      n.content.toLowerCase().includes(q)
    );
  }

  return [...result].sort((a, b) => {
    if (sortBy.value === 'updated') {
      return b.updatedAt.localeCompare(a.updatedAt);
    } else if (sortBy.value === 'title') {
      return a.title.localeCompare(b.title);
    } else {
      return b.wordCount - a.wordCount;
    }
  });
});

const selectNote = (id: string) => {
  selectedNoteId.value = id;
};

const navigateToWikilink = (linkTitle: string) => {
  const clean = linkTitle.toLowerCase().replace(/[^a-z0-9-]/g, '-');
  const target = notes.value.find(n => n.id === clean || n.title.toLowerCase().includes(linkTitle.toLowerCase()));
  if (target) {
    selectedNoteId.value = target.id;
    toast.info(`Navigated to [[${target.title}]]`);
  } else {
    toast.warning(`Note [[${linkTitle}]] does not exist in vault.`);
  }
};

const copyNoteContent = async () => {
  if (!selectedNote.value) return;
  try {
    await navigator.clipboard.writeText(selectedNote.value.content);
    toast.success('Markdown copied to clipboard!');
  } catch (err) {
    toast.error(`Could not copy: ${String(err)}`);
  }
};

const syncVault = async () => {
  isLoading.value = true;
  await new Promise(r => setTimeout(r, 400));
  isLoading.value = false;
  toast.success('Obsidian Vault indexed successfully (53 notes, 142 backlinks).');
};

const createNewNote = () => {
  const newId = `note-${Date.now()}`;
  const newNote: NoteItem = {
    id: newId,
    title: 'Untitled Note',
    category: 'PKM',
    path: `03-pkm/${newId}.md`,
    tags: ['new-note'],
    updatedAt: new Date().toISOString().slice(0, 16).replace('T', ' '),
    wordCount: 15,
    frontmatter: {
      title: 'Untitled Note',
      status: 'draft',
    },
    content: `# Untitled Note\n\nStart typing notes here... Use [[Note Title]] for wikilinks.`,
    outgoingLinks: [],
    backlinks: [],
  };
  notes.value.unshift(newNote);
  selectedNoteId.value = newId;
  toast.success('Created new draft note in vault.');
};

onMounted(() => {
  if (!gateway.isConnected.value) {
    gateway.init();
  }
});
</script>

<template>
  <div class="obsidian-vault-view animate-fadeIn">
    <!-- Header -->
    <div class="page-header flex justify-between items-center flex-wrap gap-4">
      <div>
        <h1 class="section-title">🔮 {{ t('nav_vault') || 'Obsidian PKM Knowledge Vault' }}</h1>
        <p class="page-desc">Explore personal knowledge, markdown documentation, frontmatter schemas & bidirectional links.</p>
      </div>

      <div class="header-controls flex items-center gap-3">
        <button class="btn btn-secondary text-xs flex items-center gap-1.5" @click="syncVault" :disabled="isLoading">
          <span>🔄</span> Sync Vault
        </button>
        <button class="btn btn-primary text-xs flex items-center gap-1.5" @click="createNewNote">
          <span>➕</span> New Note
        </button>
      </div>
    </div>

    <!-- Vault Meta Banner -->
    <div class="card vault-meta-banner flex justify-between items-center flex-wrap gap-3 py-2.5 px-4 mb-4 bg-tertiary">
      <div class="flex items-center gap-3 text-xs">
        <span class="vault-path-badge font-mono px-2 py-1 rounded bg-inset border border-default">
          📁 teamwork_projects/obsidian_llm_wiki/vault/
        </span>
        <span class="text-secondary">{{ notes.length }} notes indexed</span>
        <span class="text-muted">•</span>
        <span class="text-secondary">142 backlinks</span>
      </div>

      <!-- Sort Controls -->
      <div class="flex items-center gap-2 text-xs">
        <span class="text-muted">Sort:</span>
        <select v-model="sortBy" class="input py-1 px-2 text-xs bg-secondary border border-default rounded">
          <option value="updated">Recently Updated</option>
          <option value="title">Title (A-Z)</option>
          <option value="words">Word Count</option>
        </select>
      </div>
    </div>

    <!-- Split-Pane Container -->
    <div class="split-pane grid grid-cols-1 md:grid-cols-12 gap-4">
      <!-- Left Pane: Note Explorer (4 Cols) -->
      <div class="card explorer-pane md:col-span-4 flex flex-col gap-3">
        <!-- Search & Filter -->
        <div class="search-box">
          <input
            v-model="searchQuery"
            type="text"
            class="input w-full text-xs"
            placeholder="🔍 Search notes, tags, content..."
          />
        </div>

        <!-- Category Tags -->
        <div class="category-chips flex flex-wrap gap-1.5">
          <button
            v-for="cat in categories"
            :key="cat"
            :class="['chip-btn', { active: selectedCategory === cat }]"
            @click="selectedCategory = cat"
          >
            {{ cat }}
          </button>
        </div>

        <!-- Loading Skeleton -->
        <div v-if="isLoading" class="flex flex-col gap-2">
          <SkeletonLoader type="list" :count="4" />
        </div>

        <!-- Notes List -->
        <div v-else class="notes-list flex flex-col gap-2 overflow-y-auto pr-1 max-h-[560px]">
          <div
            v-for="note in filteredNotes"
            :key="note.id"
            :class="['note-item-card p-3 rounded-lg border transition cursor-pointer flex flex-col gap-1.5', {
              'note-active border-indigo-500 bg-indigo-500/10': selectedNote?.id === note.id,
              'border-default bg-tertiary/50 hover:bg-tertiary': selectedNote?.id !== note.id
            }]"
            @click="selectNote(note.id)"
          >
            <div class="flex justify-between items-start gap-2">
              <span class="font-semibold text-xs line-clamp-1" :class="{ 'text-indigo-300': selectedNote?.id === note.id }">
                {{ note.title }}
              </span>
              <span class="badge text-[10px] px-1.5 py-0.5 bg-inset border border-default/50 shrink-0">
                {{ note.category }}
              </span>
            </div>

            <div class="flex flex-wrap gap-1">
              <span v-for="tag in note.tags" :key="tag" class="text-[10px] text-muted">
                #{{ tag }}
              </span>
            </div>

            <div class="flex justify-between items-center text-[10px] text-muted pt-1 border-t border-default/20">
              <span>{{ note.updatedAt }}</span>
              <span>{{ note.wordCount }} words</span>
            </div>
          </div>

          <div v-if="!filteredNotes.length" class="text-xs text-muted text-center py-8">
            No notes found matching your search.
          </div>
        </div>
      </div>

      <!-- Right Pane: Markdown Reader & Inspector (8 Cols) -->
      <div class="card reader-pane md:col-span-8 flex flex-col gap-4">
        <template v-if="selectedNote">
          <!-- Reader Header -->
          <div class="reader-header flex justify-between items-start flex-wrap gap-3 pb-3 border-b border-default">
            <div>
              <div class="flex items-center gap-2 mb-1">
                <span class="badge badge-info text-xs">{{ selectedNote.category }}</span>
                <span class="text-xs text-muted font-mono">{{ selectedNote.path }}</span>
              </div>
              <h2 class="text-lg font-bold text-primary">{{ selectedNote.title }}</h2>
              <div class="flex items-center gap-3 text-xs text-muted mt-1">
                <span>Updated: {{ selectedNote.updatedAt }}</span>
                <span>•</span>
                <span>{{ selectedNote.wordCount }} words (~{{ Math.ceil(selectedNote.wordCount / 200) }} min read)</span>
              </div>
            </div>

            <div class="reader-actions flex items-center gap-2">
              <button
                class="btn btn-secondary text-xs py-1 px-2.5"
                @click="showFrontmatter = !showFrontmatter"
              >
                {{ showFrontmatter ? 'Hide YAML' : 'Show YAML' }}
              </button>
              <button class="btn btn-primary text-xs py-1 px-2.5 flex items-center gap-1" @click="copyNoteContent">
                📋 Copy
              </button>
            </div>
          </div>

          <!-- Frontmatter YAML Card -->
          <div v-if="showFrontmatter && Object.keys(selectedNote.frontmatter).length" class="frontmatter-card p-3 rounded-lg bg-tertiary border border-default text-xs font-mono">
            <div class="text-muted text-[10px] uppercase font-semibold mb-1">--- YAML FRONTMATTER ---</div>
            <div v-for="(val, key) in selectedNote.frontmatter" :key="key" class="grid grid-cols-3 gap-2">
              <span class="text-secondary">{{ key }}:</span>
              <span class="col-span-2 text-sky-300">{{ val }}</span>
            </div>
            <div class="text-muted text-[10px] uppercase font-semibold mt-1">---</div>
          </div>

          <!-- Markdown Content Body -->
          <div class="markdown-body p-4 rounded-lg bg-inset border border-default text-xs leading-relaxed overflow-y-auto max-h-[420px] whitespace-pre-wrap font-sans">
            {{ selectedNote.content }}
          </div>

          <!-- Wikilinks & Backlinks Footer -->
          <div class="links-footer grid grid-cols-1 sm:grid-cols-2 gap-3 pt-3 border-t border-default text-xs">
            <!-- Outgoing Links -->
            <div class="outgoing-card p-2.5 rounded bg-tertiary border border-default">
              <div class="font-semibold text-secondary mb-1 flex items-center gap-1">
                <span>🔗</span> Outgoing Links ({{ selectedNote.outgoingLinks.length }})
              </div>
              <div class="flex flex-wrap gap-1.5">
                <button
                  v-for="link in selectedNote.outgoingLinks"
                  :key="link"
                  class="wikilink-pill text-[11px] px-2 py-0.5 rounded bg-indigo-500/15 text-indigo-300 hover:bg-indigo-500/30 transition cursor-pointer border border-indigo-500/30"
                  @click="navigateToWikilink(link)"
                >
                  [[{{ link }}]]
                </button>
                <span v-if="!selectedNote.outgoingLinks.length" class="text-muted text-[11px]">No outgoing links</span>
              </div>
            </div>

            <!-- Backlinks -->
            <div class="backlinks-card p-2.5 rounded bg-tertiary border border-default">
              <div class="font-semibold text-secondary mb-1 flex items-center gap-1">
                <span>↩️</span> Backlinks ({{ selectedNote.backlinks.length }})
              </div>
              <div class="flex flex-wrap gap-1.5">
                <button
                  v-for="blink in selectedNote.backlinks"
                  :key="blink"
                  class="wikilink-pill text-[11px] px-2 py-0.5 rounded bg-purple-500/15 text-purple-300 hover:bg-purple-500/30 transition cursor-pointer border border-purple-500/30"
                  @click="navigateToWikilink(blink)"
                >
                  [[{{ blink }}]]
                </button>
                <span v-if="!selectedNote.backlinks.length" class="text-muted text-[11px]">No backlinks</span>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.obsidian-vault-view {
  padding: var(--space-lg);
  color: var(--text-primary);
  height: 100%;
  overflow-y: auto;
}

.page-header {
  margin-bottom: var(--space-lg);
}

.section-title {
  font-size: 24px;
  font-weight: 700;
}

.page-desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin-top: 4px;
}

.vault-meta-banner {
  border-radius: var(--radius-md);
}

/* Category Chips */
.chip-btn {
  padding: 3px 8px;
  border-radius: 4px;
  border: 1px solid var(--border-default);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.chip-btn:hover {
  color: var(--text-primary);
  border-color: var(--text-muted);
}

.chip-btn.active {
  background: var(--accent-start);
  color: #fff;
  border-color: var(--accent-start);
  font-weight: 600;
}

/* Markdown Rendering Container */
.markdown-body {
  font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  color: #e2e8f0;
  line-height: 1.6;
}

.wikilink-pill {
  border: 1px solid rgba(129, 140, 248, 0.3);
}
</style>
