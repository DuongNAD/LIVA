<template>
  <div class="memory-task-board">
    <div class="board-tabs">
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'tasks' }" 
        @click="activeTab = 'tasks'"
      >
        Task Planner
      </button>
      <button 
        class="tab-btn" 
        :class="{ active: activeTab === 'memory' }" 
        @click="activeTab = 'memory'"
      >
        Memory Inspector
      </button>
    </div>

    <!-- TAB 1: Task Planner (Split Screen Layout) -->
    <div v-if="activeTab === 'tasks'" class="tasks-panel">
      <div class="tasks-sub-tabs">
        <button 
          class="sub-tab-btn" 
          :class="{ active: tasksSubTab === 'list' }" 
          @click="tasksSubTab = 'list'"
        >
          Task List
        </button>
        <button 
          class="sub-tab-btn" 
          :class="{ active: tasksSubTab === 'chat' }" 
          @click="tasksSubTab = 'chat'"
        >
          Chat Planner
        </button>
      </div>
      <div class="split-container">
        <!-- Left Column: Tasks List & Controls -->
        <div class="tasks-list-col" :class="{ 'hidden-mobile': tasksSubTab !== 'list' }">
          <div class="panel-section-header">
            <h3>Active Tasks</h3>
            <button class="icon-btn" @click="promptAddTask">+</button>
          </div>
          <div class="tasks-scroll-container">
            <div 
              v-for="task in tasks" 
              :key="task.id" 
              class="task-item" 
              :class="{ completed: task.completed, selected: selectedTaskId === task.id }"
              @click="selectedTaskId = task.id"
            >
              <input 
                type="checkbox" 
                :checked="task.completed" 
                @change.stop="toggleTask(task)"
              />
              <span class="task-title">{{ task.title }}</span>
              <button class="delete-btn" @click.stop="deleteTask(task.id)">×</button>
            </div>
            <div v-if="tasks.length === 0" class="empty-state">
              No tasks found. Click '+' to add.
            </div>
          </div>
        </div>

        <!-- Right Column: Interactive Chat Planner -->
        <div class="chat-planner-col" :class="{ 'hidden-mobile': tasksSubTab !== 'chat' }">
          <div class="panel-section-header">
            <h3>Task Chat Planner</h3>
          </div>
          <div class="chat-log" ref="chatLogRef">
            <div 
              v-for="(msg, index) in plannerChatLog" 
              :key="index" 
              class="chat-bubble"
              :class="msg.role"
            >
              <div class="bubble-sender">{{ msg.role === 'user' ? 'You' : 'Planner AI' }}</div>
              <div class="bubble-text">{{ msg.text }}</div>
            </div>
            <div v-if="plannerChatLog.length === 0" class="empty-state">
              Select a task and chat here to refine it.
            </div>
          </div>
          <div class="chat-input-row">
            <input 
              v-model="plannerInput" 
              type="text" 
              placeholder="Refine task details..." 
              @keyup.enter="sendPlannerChat"
            />
            <button @click="sendPlannerChat">Send</button>
          </div>
        </div>
      </div>
    </div>

    <!-- TAB 2: Memory Inspector (Three-Tiered Memory Display) -->
    <div v-if="activeTab === 'memory'" class="memory-panel">
      <!-- Top Tier: Facts (Key-Value Facts extracted by AI) -->
      <div class="memory-section">
        <h4 class="section-title">Facts (L1 Core Knowledge)</h4>
        <div class="facts-grid">
          <div v-for="(val, key) in facts" :key="key" class="fact-card">
            <span class="fact-key">{{ key }}</span>
            <span class="fact-val">{{ val }}</span>
          </div>
          <div v-if="Object.keys(facts).length === 0" class="empty-state">
            No memory facts established yet.
          </div>
        </div>
        <div class="add-fact-row">
          <input v-model="newFactKey" type="text" placeholder="Key" />
          <input v-model="newFactVal" type="text" placeholder="Value" />
          <button @click="addFact">Add Fact</button>
        </div>
      </div>

      <!-- Middle Tier: Turn Layer (L0 recent context nodes) -->
      <div class="memory-section">
        <h4 class="section-title">Turn Layer (L0 Context)</h4>
        <div class="turn-list">
          <div v-for="(node, index) in turnLayer" :key="index" class="turn-node">
            <span class="turn-time">[{{ node.timestamp }}]</span>
            <strong class="turn-role">{{ node.role.toUpperCase() }}:</strong>
            <span class="turn-text">{{ node.text }}</span>
          </div>
          <div v-if="turnLayer.length === 0" class="empty-state">
            No recent interaction turns in context.
          </div>
        </div>
      </div>

      <!-- Bottom Tier: Events / Vectors (Semantic Search interface) -->
      <div class="memory-section">
        <h4 class="section-title">Semantic Event Search (L2 Episodic Memory)</h4>
        <div class="search-row">
          <input v-model="searchQuery" type="text" placeholder="Search past events/conversations..." />
          <button @click="searchMemory">Search</button>
        </div>
        <div class="search-results">
          <div v-for="(res, index) in searchResults" :key="index" class="result-card">
            <div class="result-meta">Score: {{ res.score.toFixed(3) }} | {{ res.timestamp }}</div>
            <div class="result-content">{{ res.content }}</div>
          </div>
          <div v-if="searchResults.length === 0 && searchQuery" class="empty-state">
            No matching events found.
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts">
import { defineComponent, ref, nextTick } from 'vue'
import { logger } from '../services/logger'

interface Task {
  id: string;
  title: string;
  completed: boolean;
}

interface ChatMessage {
  role: 'user' | 'assistant';
  text: string;
}

interface TurnNode {
  timestamp: string;
  role: string;
  text: string;
}

interface SearchResult {
  score: number;
  timestamp: string;
  content: string;
}

export default defineComponent({
  name: 'MemoryTaskBoard',
  props: {
    wsClient: {
      type: Object, // WebSocketClient instance
      required: false
    }
  },
  setup(props) {
    const activeTab = ref<'tasks' | 'memory'>('tasks')
    const tasksSubTab = ref<'list' | 'chat'>('list')
    
    // --- Task Planner State ---
    const tasks = ref<Task[]>([
      { id: '1', title: 'Complete Mobile Client implementation', completed: false },
      { id: '2', title: 'Verify connection using handshake script', completed: false },
      { id: '3', title: 'Optimize S24+ layout metrics', completed: true }
    ])
    const selectedTaskId = ref<string>('1')
    const plannerInput = ref('')
    const plannerChatLog = ref<ChatMessage[]>([
      { role: 'assistant', text: 'How would you like to refine the active tasks?' }
    ])
    const chatLogRef = ref<HTMLElement | null>(null)

    // --- Memory State ---
    const facts = ref<Record<string, string>>({
      'hobbies': 'Học AI',
      'device': 'Samsung S24+',
      'framework': 'Capacitor 8 / Vue 3'
    })
    const newFactKey = ref('')
    const newFactVal = ref('')
    const turnLayer = ref<TurnNode[]>([
      { timestamp: '11:45:10', role: 'user', text: 'Hello, what is my device model?' },
      { timestamp: '11:45:12', role: 'assistant', text: 'You are using a Samsung S24+ device.' }
    ])
    const searchQuery = ref('')
    const searchResults = ref<SearchResult[]>([])

    // --- Task Planner Actions ---
    const promptAddTask = () => {
      const title = prompt('Enter task description:')
      if (title && title.trim()) {
        const newTask: Task = {
          id: Math.random().toString(36).substring(2),
          title: title.trim(),
          completed: false
        }
        tasks.value.push(newTask)
        if (props.wsClient && props.wsClient.isConnected()) {
          props.wsClient.sendJsonCommand('add_task', { title: newTask.title })
            .catch((e: unknown) => logger.error('WS Error:', e))
        }
      }
    }

    const toggleTask = (task: Task) => {
      task.completed = !task.completed
      if (props.wsClient && props.wsClient.isConnected()) {
        props.wsClient.sendJsonCommand('update_task', { id: task.id, completed: task.completed })
          .catch((e: unknown) => logger.error('WS Error:', e))
      }
    }

    const deleteTask = (id: string) => {
      tasks.value = tasks.value.filter(t => t.id !== id)
      if (props.wsClient && props.wsClient.isConnected()) {
        props.wsClient.sendJsonCommand('delete_task', { id })
          .catch((e: unknown) => logger.error('WS Error:', e))
      }
    }

    const sendPlannerChat = async () => {
      if (!plannerInput.value.trim()) return
      const userText = plannerInput.value.trim()
      plannerChatLog.value.push({ role: 'user', text: userText })
      plannerInput.value = ''
      await nextTick()
      scrollChat()

      // Call WebSocket server task_plan_chat
      if (props.wsClient && props.wsClient.isConnected()) {
        props.wsClient.sendJsonCommand('task_plan_chat', {
          task_id: selectedTaskId.value,
          message: userText
        }).then((data: unknown) => {
          const res = data as { reply?: string }
          plannerChatLog.value.push({
            role: 'assistant',
            text: res.reply || 'Task refined successfully.'
          })
          nextTick().then(scrollChat)
        }).catch((err: unknown) => {
          const errMsg = err instanceof Error ? err.message : String(err)
          plannerChatLog.value.push({
            role: 'assistant',
            text: `Error communicating with planner backend: ${errMsg}`
          })
          nextTick().then(scrollChat)
        })
      } else {
        // Mock Response if not connected
        setTimeout(() => {
          plannerChatLog.value.push({
            role: 'assistant',
            text: `Mock Planner response for: "${userText}"`
          })
          nextTick().then(scrollChat)
        }, 800)
      }
    }

    const scrollChat = () => {
      if (chatLogRef.value) {
        chatLogRef.value.scrollTop = chatLogRef.value.scrollHeight
      }
    }

    // --- Memory Actions ---
    const addFact = () => {
      if (newFactKey.value.trim() && newFactVal.value.trim()) {
        const k = newFactKey.value.trim()
        const v = newFactVal.value.trim()
        facts.value[k] = v
        
        if (props.wsClient && props.wsClient.isConnected()) {
          props.wsClient.sendJsonCommand('memory:set_fact', { key: k, value: v })
            .catch((e: unknown) => logger.error('WS Error:', e))
        }

        newFactKey.value = ''
        newFactVal.value = ''
      }
    }

    const searchMemory = () => {
      if (!searchQuery.value.trim()) return
      
      if (props.wsClient && props.wsClient.isConnected()) {
        props.wsClient.sendJsonCommand('memory:search_hybrid', { query: searchQuery.value })
          .then((data: unknown) => {
            const res = data as { results?: SearchResult[] }
            searchResults.value = res.results || []
          })
          .catch((err: unknown) => {
            logger.error('Search failed:', err)
          })
      } else {
        // Mock search results
        searchResults.value = [
          { score: 0.892, timestamp: '2026-06-24 14:22', content: `Found key reference containing "${searchQuery.value}" in long-term memory.` },
          { score: 0.724, timestamp: '2026-06-25 09:10', content: `Session event logs matched search query term.` }
        ]
      }
    }

    return {
      activeTab,
      tasksSubTab,
      tasks,
      selectedTaskId,
      plannerInput,
      plannerChatLog,
      chatLogRef,
      facts,
      newFactKey,
      newFactVal,
      turnLayer,
      searchQuery,
      searchResults,
      promptAddTask,
      toggleTask,
      deleteTask,
      sendPlannerChat,
      addFact,
      searchMemory
    }
  }
})
</script>

<style scoped>
.memory-task-board {
  background: #111827;
  border-radius: 16px;
  border: 1px solid #1f2937;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  height: 100%;
}

.board-tabs {
  display: flex;
  background-color: #1f2937;
  border-bottom: 1px solid #374151;
}

.tab-btn {
  flex: 1;
  background: none;
  border: none;
  outline: none;
  padding: 14px;
  color: #9ca3af;
  font-weight: 600;
  font-size: 0.85rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.tab-btn.active {
  color: white;
  background-color: #111827;
  box-shadow: inset 0 -2px 0 #6366f1;
}

.tasks-sub-tabs {
  display: none;
}

.tasks-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.split-container {
  display: flex;
  flex: 1;
  height: 100%;
  overflow: hidden;
}

.tasks-list-col {
  width: 45%;
  border-right: 1px solid #1f2937;
  display: flex;
  flex-direction: column;
  background-color: #111827;
}

.chat-planner-col {
  width: 55%;
  display: flex;
  flex-direction: column;
  background-color: #0d1117;
}

@media (max-width: 768px) {
  .split-container {
    flex-direction: column;
  }
  .tasks-list-col {
    width: 100%;
    height: 100%;
    border: none;
  }
  .chat-planner-col {
    width: 100%;
    height: 100%;
    border: none;
  }
  .tasks-sub-tabs {
    display: flex;
    background-color: #1f2937;
    border-bottom: 1px solid #374151;
  }
  .tasks-sub-tabs .sub-tab-btn {
    flex: 1;
    background: none;
    border: none;
    outline: none;
    padding: 12px;
    color: #9ca3af;
    font-weight: 600;
    font-size: 0.85rem;
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: center;
  }
  .tasks-sub-tabs .sub-tab-btn.active {
    color: white;
    background-color: #111827;
    box-shadow: inset 0 -2px 0 #6366f1;
  }
  .hidden-mobile {
    display: none !important;
  }
}

.panel-section-header {
  padding: 12px;
  border-bottom: 1px solid #1f2937;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.panel-section-header h3 {
  margin: 0;
  font-size: 0.8rem;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.icon-btn {
  background: #374151;
  border: none;
  color: white;
  width: 24px;
  height: 24px;
  border-radius: 4px;
  font-weight: bold;
  cursor: pointer;
  display: flex;
  justify-content: center;
  align-items: center;
}

.tasks-scroll-container {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.task-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px;
  border-radius: 8px;
  background: #1f2937;
  margin-bottom: 8px;
  cursor: pointer;
  transition: background-color 0.2s;
  position: relative;
}

.task-item:hover, .task-item.selected {
  background: #374151;
}

.task-item.selected {
  border-left: 3px solid #6366f1;
}

.task-item.completed .task-title {
  text-decoration: line-through;
  color: #6b7280;
}

.task-title {
  font-size: 0.8rem;
  color: #e5e7eb;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.delete-btn {
  background: none;
  border: none;
  color: #ef4444;
  font-size: 1.2rem;
  cursor: pointer;
  margin-left: auto;
}

.chat-log {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.chat-bubble {
  max-width: 85%;
  padding: 10px 12px;
  border-radius: 12px;
  font-size: 0.8rem;
  line-height: 1.4;
}

.chat-bubble.user {
  background-color: #6366f1;
  color: white;
  align-self: flex-end;
  border-bottom-right-radius: 2px;
}

.chat-bubble.assistant {
  background-color: #1f2937;
  color: #f3f4f6;
  align-self: flex-start;
  border-bottom-left-radius: 2px;
}

.bubble-sender {
  font-size: 0.65rem;
  color: rgba(255, 255, 255, 0.6);
  margin-bottom: 4px;
}

.chat-input-row {
  padding: 10px;
  border-top: 1px solid #1f2937;
  display: flex;
  gap: 8px;
  background-color: #111827;
}

.chat-input-row input {
  flex: 1;
  background: #1f2937;
  border: 1px solid #374151;
  color: white;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 0.8rem;
  outline: none;
}

.chat-input-row button {
  background: #6366f1;
  border: none;
  color: white;
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 0.8rem;
  font-weight: 600;
  cursor: pointer;
}

/* Memory panel details */
.memory-panel {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.memory-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.section-title {
  margin: 0;
  font-size: 0.75rem;
  color: #8b5cf6;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-left: 2px solid #8b5cf6;
  padding-left: 8px;
}

.facts-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  gap: 8px;
}

.fact-card {
  background: #1f2937;
  border: 1px solid #374151;
  border-radius: 8px;
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
}

.fact-key {
  font-size: 0.6rem;
  color: #9ca3af;
  text-transform: uppercase;
}

.fact-val {
  font-size: 0.8rem;
  color: #f3f4f6;
  font-weight: 600;
  margin-top: 4px;
}

.add-fact-row {
  display: flex;
  gap: 8px;
}

.add-fact-row input {
  flex: 1;
  background: #1f2937;
  border: 1px solid #374151;
  color: white;
  padding: 6px 10px;
  border-radius: 6px;
  font-size: 0.75rem;
  outline: none;
}

.add-fact-row button {
  background: #8b5cf6;
  border: none;
  color: white;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 0.75rem;
  cursor: pointer;
}

.turn-list {
  background: #0d1117;
  border: 1px solid #1f2937;
  border-radius: 8px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.turn-node {
  font-size: 0.75rem;
  line-height: 1.4;
}

.turn-time {
  color: #6b7280;
  margin-right: 6px;
}

.turn-role {
  color: #3b82f6;
  margin-right: 6px;
}

.turn-text {
  color: #d1d5db;
}

.search-row {
  display: flex;
  gap: 8px;
}

.search-row input {
  flex: 1;
  background: #1f2937;
  border: 1px solid #374151;
  color: white;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 0.8rem;
  outline: none;
}

.search-row button {
  background: #3b82f6;
  border: none;
  color: white;
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 0.8rem;
  cursor: pointer;
}

.search-results {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 8px;
}

.result-card {
  background: #1f2937;
  border-left: 3px solid #10b981;
  border-radius: 6px;
  padding: 10px;
}

.result-meta {
  font-size: 0.6rem;
  color: #9ca3af;
  margin-bottom: 4px;
}

.result-content {
  font-size: 0.75rem;
  color: #e5e7eb;
}

.empty-state {
  text-align: center;
  color: #4b5563;
  font-size: 0.75rem;
  padding: 20px;
}
</style>
