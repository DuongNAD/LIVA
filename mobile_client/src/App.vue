<template>
  <div class="app-layout">
    <!-- Top Header / Connection Panel -->
    <header class="app-header">
      <h1 class="logo">LIVA</h1>
      <div class="conn-controls">
        <input 
          v-model="wsUrl" 
          type="text" 
          placeholder="ws://127.0.0.1:8002/ws" 
          class="url-input"
          :disabled="connected"
        />
        <button 
          @click="toggleConnection" 
          class="conn-btn" 
          :class="{ connected }"
        >
          {{ connected ? 'Disconnect' : 'Connect' }}
        </button>
      </div>
    </header>

    <!-- Main Content Area -->
    <main class="app-main">
      <!-- Upper part: Avatar View (optimized for top 60% of interaction screen) -->
      <section class="avatar-section">
        <AvatarScreen 
          :subtitle="subtitle" 
          :is-talking="isTalking" 
          @mic-start="handleMicStart" 
          @mic-stop="handleMicStop"
          @expand-menu="toggleBoard"
        />
      </section>

      <!-- Middle part: Latency and diagnostics widget -->
      <section class="metrics-section">
        <LatencyMetrics 
          :ping-latency="pingLatency" 
          :vad-active="vadActive" 
          :is-connected="connected"
        />
      </section>

      <!-- Bottom part: Expandable Memory and Task Planner Drawer/Board -->
      <section class="board-section" :class="{ expanded: boardExpanded }">
        <MemoryTaskBoard :ws-client="wsClient || undefined" />
      </section>
    </main>
  </div>
</template>

<script lang="ts">
import { defineComponent, ref, onBeforeUnmount } from 'vue'
import { WebSocketClient, OpCode } from './services/WebSocketClient'
import { logger } from './services/logger'
import AvatarScreen from './components/AvatarScreen.vue'
import LatencyMetrics from './components/LatencyMetrics.vue'
import MemoryTaskBoard from './components/MemoryTaskBoard.vue'

export default defineComponent({
  name: 'App',
  components: {
    AvatarScreen,
    LatencyMetrics,
    MemoryTaskBoard
  },
  setup() {
    const wsUrl = ref('ws://127.0.0.1:8002/ws')
    const connected = ref(false)
    const subtitle = ref('System ready. Press Connect to start.')
    const isTalking = ref(false)
    const pingLatency = ref<number | undefined>(undefined)
    const vadActive = ref(false)
    const boardExpanded = ref(true)

    let wsClient: WebSocketClient | null = null
    let pingInterval: number | null = null
    let micSimulationInterval: number | null = null
    let micSeqId = 0

    const toggleConnection = async () => {
      if (connected.value) {
        disconnect()
      } else {
        await connect()
      }
    }

    const connect = async () => {
      try {
        subtitle.value = 'Connecting to server...'
        wsClient = new WebSocketClient(wsUrl.value)

        // Setup handlers
        wsClient.onConnect(() => {
          connected.value = true
          subtitle.value = 'Connected. Performing authentication handshake...'
          
          // Trigger the OP_AUTH_HANDSHAKE
          wsClient?.sendAuthHandshake(100, 'device_s24_token')
            .then((handshakeFrame) => {
              subtitle.value = 'Handshake verified. System active.'
              logger.log('Handshake frame response verified:', handshakeFrame)
              
              // Start ping metrics loop
              startPingMetrics()
            })
            .catch((err: unknown) => {
              logger.error('Handshake failed:', err)
              subtitle.value = 'Authentication handshake failed.'
            })
        })

        wsClient.onDisconnect(() => {
          connected.value = false
          subtitle.value = 'Connection lost. Tap Connect to try again.'
          stopPingMetrics()
          stopMicSimulation()
        })

        wsClient.onError((err: Event) => {
          logger.error('WebSocket connection error:', err)
          subtitle.value = 'Connection error.'
        })

        wsClient.onVoiceFrame((frame) => {
          logger.log(`Received binary frame: OpCode 0x${frame.opcode.toString(16)}, SeqId ${frame.seqId}`)
          if (frame.opcode === OpCode.OP_SPEAKER_OUT) {
            isTalking.value = true
            // Convert simple text payload or show speaking subtitle
            subtitle.value = 'AI Speaking (Audio streaming active)'
            
            // Auto turn-off talk avatar after delay
            setTimeout(() => {
              isTalking.value = false
            }, 2000)
          } else if (frame.opcode === OpCode.OP_FLUSH) {
            isTalking.value = false
            subtitle.value = 'Speaker playback flushed.'
          }
        })

        await wsClient.connect()
      } catch (err: unknown) {
        logger.error('Failed to connect:', err)
        subtitle.value = 'Failed to connect to backend.'
      }
    }

    const disconnect = () => {
      if (wsClient) {
        wsClient.disconnect()
        wsClient = null
      }
      connected.value = false
      stopPingMetrics()
      stopMicSimulation()
    }

    const startPingMetrics = () => {
      // Periodic ping check using the JSON control channel
      pingInterval = window.setInterval(async () => {
        if (wsClient && wsClient.isConnected()) {
          const startTime = performance.now()
          try {
            const res = await wsClient.sendJsonCommand('ping') as { pong?: boolean }
            if (res && res.pong) {
              const endTime = performance.now()
              pingLatency.value = Math.round(endTime - startTime)
            }
          } catch (err: unknown) {
            logger.error('Ping command failed:', err)
            pingLatency.value = undefined
          }
        }
      }, 3000)
    }

    const stopPingMetrics = () => {
      if (pingInterval) {
        clearInterval(pingInterval)
        pingInterval = null
      }
      pingLatency.value = undefined
    }

    const handleMicStart = () => {
      vadActive.value = true
      subtitle.value = 'Listening... (Streaming audio to server)'
      
      // Simulate sending 32-bit float PCM audio samples via OP_MIC_IN
      if (wsClient && wsClient.isConnected()) {
        micSeqId = 1
        micSimulationInterval = window.setInterval(() => {
          if (wsClient && wsClient.isConnected()) {
            // Generate dummy 16kHz, mono 32-bit PCM data (640 samples = 2560 bytes)
            const floatSamples = new Float32Array(640)
            for (let i = 0; i < floatSamples.length; i++) {
              floatSamples[i] = Math.sin(i * 0.1) * 0.1 // Dummy sine wave
            }
            const bytePayload = new Uint8Array(floatSamples.buffer)
            wsClient.sendVoiceFrame(OpCode.OP_MIC_IN, micSeqId++, bytePayload)
          }
        }, 40) // 40ms frame size
      }
    }

    const handleMicStop = () => {
      vadActive.value = false
      subtitle.value = 'Audio stream complete. Waiting for processing...'
      stopMicSimulation()
      
      // Request simple voice stop if connected
      if (wsClient && wsClient.isConnected()) {
        wsClient.sendJsonCommand('voice:stt_stop')
          .then((res: unknown) => {
            const data = res as { transcript?: string }
            subtitle.value = data.transcript ? `You: "${data.transcript}"` : 'Processing response...'
          })
          .catch((err: unknown) => {
            logger.error('Voice stop failed:', err)
          })
      }
    }

    const stopMicSimulation = () => {
      if (micSimulationInterval) {
        clearInterval(micSimulationInterval)
        micSimulationInterval = null
      }
    }

    const toggleBoard = () => {
      boardExpanded.value = !boardExpanded.value
    }

    onBeforeUnmount(() => {
      disconnect()
    })

    return {
      wsUrl,
      connected,
      subtitle,
      isTalking,
      pingLatency,
      vadActive,
      boardExpanded,
      wsClient,
      toggleConnection,
      handleMicStart,
      handleMicStop,
      toggleBoard
    }
  }
})
</script>

<style scoped>
.app-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  background-color: #0c0f16;
  overflow: hidden;
  padding-top: env(safe-area-inset-top);
  padding-bottom: env(safe-area-inset-bottom);
  box-sizing: border-box;
}

.app-header {
  background-color: #111827;
  border-bottom: 1px solid #1f2937;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  height: 50px;
  box-sizing: border-box;
}

.logo {
  margin: 0;
  font-size: 1.1rem;
  font-weight: 800;
  color: #6366f1;
  letter-spacing: 0.1em;
}

.conn-controls {
  display: flex;
  align-items: center;
  gap: 8px;
}

.url-input {
  background-color: #1f2937;
  border: 1px solid #374151;
  color: #f3f4f6;
  font-size: 0.75rem;
  padding: 6px 10px;
  border-radius: 6px;
  width: 150px;
  outline: none;
}

.conn-btn {
  background-color: #6366f1;
  border: none;
  color: white;
  font-size: 0.75rem;
  font-weight: 600;
  padding: 6px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.conn-btn.connected {
  background-color: #ef4444;
}

.app-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

.avatar-section {
  flex: 1;
  min-height: 0;
}

.metrics-section {
  padding: 0 16px 12px 16px;
}

.board-section {
  height: 38%;
  padding: 0 16px 16px 16px;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  box-sizing: border-box;
}

.board-section.expanded {
  height: 42%;
}
</style>
