<template>
  <div class="avatar-screen">
    <!-- Top 60%: Avatar Container -->
    <div class="avatar-container" @pointermove="onPointerMove">
      <div class="avatar-placeholder">
        <!-- Interactive visual placeholder for 3D VRM / 2D Live2D -->
        <div class="avatar-glow" :style="avatarGlowStyle"></div>
        <div class="avatar-eyes" :style="eyesStyle">
          <span class="eye left"></span>
          <span class="eye right"></span>
        </div>
        <div class="avatar-mouth" :class="{ talking: isTalking }"></div>
        <div class="avatar-tag">{{ avatarMode === '3d' ? '3D VRM Model' : '2D Live2D Model' }}</div>
      </div>

      <!-- Center: Floating Subtitle Overlay -->
      <div class="subtitle-overlay" v-if="subtitle">
        <p class="subtitle-text">{{ subtitle }}</p>
      </div>
    </div>

    <!-- Bottom 40%: Controls & Radial Mic -->
    <div class="controls-container">
      <!-- Visualizer ring showing voice activity levels -->
      <div class="mic-section">
        <div class="visualizer-ring" :class="{ active: isRecording }" :style="visualizerStyle"></div>
        <button 
          class="mic-button" 
          :class="{ recording: isRecording }" 
          @mousedown="startRecording" 
          @mouseup="stopRecording"
          @touchstart.prevent="startRecording"
          @touchend.prevent="stopRecording"
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="mic-icon">
            <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
            <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
          </svg>
        </button>
        <span class="mic-hint">{{ isRecording ? 'Listening...' : 'Hold to Speak' }}</span>
      </div>

      <div class="control-toggles">
        <button class="toggle-btn" @click="toggleAvatarMode">
          {{ avatarMode.toUpperCase() }}
        </button>
        <button class="toggle-btn" :class="{ active: isMuted }" @click="toggleMute">
          {{ isMuted ? 'UNMUTE' : 'MUTE' }}
        </button>
        <button class="toggle-btn" @click="$emit('expand-menu')">
          MENU
        </button>
      </div>
    </div>
  </div>
</template>

<script lang="ts">
import { defineComponent, ref, computed } from 'vue'

export default defineComponent({
  name: 'AvatarScreen',
  props: {
    subtitle: {
      type: String,
      default: ''
    },
    isTalking: {
      type: Boolean,
      default: false
    }
  },
  emits: ['mic-start', 'mic-stop', 'expand-menu'],
  setup(props, { emit }) {
    const avatarMode = ref<'2d' | '3d'>('3d')
    const isMuted = ref(false)
    const isRecording = ref(false)
    const pointerPos = ref({ x: 0, y: 0 })
    const audioLevel = ref(0)
    let audioInterval: number | null = null

    const toggleAvatarMode = () => {
      avatarMode.value = avatarMode.value === '3d' ? '2d' : '3d'
    }

    const toggleMute = () => {
      isMuted.value = !isMuted.value
    }

    const startRecording = () => {
      if (isMuted.value) return
      isRecording.value = true
      emit('mic-start')
      
      // Simulate voice input levels for visualizer ring
      audioInterval = window.setInterval(() => {
        audioLevel.value = 0.5 + Math.random() * 0.5
      }, 100)
    }

    const stopRecording = () => {
      isRecording.value = false
      emit('mic-stop')
      if (audioInterval) {
        clearInterval(audioInterval)
        audioInterval = null
      }
      audioLevel.value = 0
    }

    const onPointerMove = (event: PointerEvent) => {
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
      const x = (event.clientX - rect.left) / rect.width - 0.5
      const y = (event.clientY - rect.top) / rect.height - 0.5
      pointerPos.value = { x, y }
    }

    const eyesStyle = computed(() => {
      const tx = pointerPos.value.x * 20
      const ty = pointerPos.value.y * 20
      return {
        transform: `translate(${tx}px, ${ty}px)`
      }
    })

    const avatarGlowStyle = computed(() => {
      if (props.isTalking) {
        return {
          boxShadow: '0 0 40px 10px rgba(99, 102, 241, 0.6)'
        }
      }
      return {}
    })

    const visualizerStyle = computed(() => {
      const scale = 1 + audioLevel.value * 0.4
      return {
        transform: `scale(${scale})`,
        opacity: isRecording.value ? '0.8' : '0.2'
      }
    })

    return {
      avatarMode,
      isMuted,
      isRecording,
      toggleAvatarMode,
      toggleMute,
      startRecording,
      stopRecording,
      onPointerMove,
      eyesStyle,
      avatarGlowStyle,
      visualizerStyle
    }
  }
})
</script>

<style scoped>
.avatar-screen {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  position: relative;
  background-color: #0d1117;
}

.avatar-container {
  height: 60%;
  position: relative;
  display: flex;
  justify-content: center;
  align-items: center;
  cursor: pointer;
  overflow: hidden;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.avatar-placeholder {
  width: 160px;
  height: 160px;
  border-radius: 50%;
  background: radial-gradient(circle, #1f2937 0%, #111827 100%);
  border: 2px solid #374151;
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  transition: all 0.3s ease;
}

.avatar-glow {
  position: absolute;
  top: -5px;
  left: -5px;
  right: -5px;
  bottom: -5px;
  border-radius: 50%;
  z-index: 0;
  pointer-events: none;
  transition: box-shadow 0.2s ease;
}

.avatar-eyes {
  display: flex;
  gap: 24px;
  z-index: 1;
  transition: transform 0.1s ease-out;
}

.eye {
  width: 16px;
  height: 16px;
  background-color: #6366f1;
  border-radius: 50%;
  display: inline-block;
  box-shadow: 0 0 10px #6366f1;
}

.avatar-mouth {
  width: 20px;
  height: 4px;
  background-color: #f3f4f6;
  border-radius: 2px;
  margin-top: 16px;
  z-index: 1;
  transition: height 0.1s ease;
}

.avatar-mouth.talking {
  animation: talk 0.15s infinite alternate;
}

@keyframes talk {
  0% { height: 4px; }
  100% { height: 16px; border-radius: 8px; }
}

.avatar-tag {
  position: absolute;
  bottom: 12px;
  font-size: 0.65rem;
  color: #9ca3af;
  letter-spacing: 0.05em;
  background: rgba(0, 0, 0, 0.4);
  padding: 2px 8px;
  border-radius: 10px;
}

.subtitle-overlay {
  position: absolute;
  bottom: 20px;
  left: 20px;
  right: 20px;
  background: rgba(15, 23, 42, 0.8);
  backdrop-filter: blur(8px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  padding: 12px 18px;
  border-radius: 12px;
  text-align: center;
  box-shadow: 0 4px 20px rgba(0,0,0,0.3);
}

.subtitle-text {
  margin: 0;
  font-size: 0.95rem;
  line-height: 1.4;
  color: #e2e8f0;
}

.controls-container {
  height: 40%;
  display: flex;
  flex-direction: column;
  justify-content: space-around;
  align-items: center;
  padding: 16px;
  background: linear-gradient(180deg, rgba(13,17,23,0) 0%, rgba(13,17,23,0.95) 100%);
}

.mic-section {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}

.visualizer-ring {
  position: absolute;
  width: 88px;
  height: 88px;
  border-radius: 50%;
  border: 4px solid #6366f1;
  box-shadow: 0 0 20px rgba(99, 102, 241, 0.4);
  pointer-events: none;
  transition: transform 0.1s ease-out, opacity 0.2s ease;
}

.visualizer-ring.active {
  border-color: #ef4444;
  box-shadow: 0 0 30px rgba(239, 68, 68, 0.6);
}

.mic-button {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  background: radial-gradient(circle, #6366f1 0%, #4f46e5 100%);
  border: none;
  outline: none;
  cursor: pointer;
  display: flex;
  justify-content: center;
  align-items: center;
  box-shadow: 0 10px 25px rgba(79, 70, 229, 0.4);
  transition: all 0.2s ease;
  z-index: 1;
}

.mic-button.recording {
  background: radial-gradient(circle, #ef4444 0%, #dc2626 100%);
  box-shadow: 0 10px 25px rgba(220, 38, 38, 0.4);
  transform: scale(0.95);
}

.mic-icon {
  width: 32px;
  height: 32px;
  color: white;
}

.mic-hint {
  margin-top: 10px;
  font-size: 0.75rem;
  color: #9ca3af;
  letter-spacing: 0.05em;
  font-weight: 500;
}

.control-toggles {
  display: flex;
  gap: 16px;
  width: 100%;
  max-width: 320px;
  justify-content: space-between;
}

.toggle-btn {
  flex: 1;
  background: #1f2937;
  border: 1px solid #374151;
  color: #d1d5db;
  padding: 10px 16px;
  border-radius: 8px;
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: center;
}

.toggle-btn:active {
  transform: scale(0.95);
  background: #374151;
}

.toggle-btn.active {
  background: #ef4444;
  border-color: #ef4444;
  color: white;
}
</style>
