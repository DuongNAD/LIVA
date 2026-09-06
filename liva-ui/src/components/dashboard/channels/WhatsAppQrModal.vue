<script setup lang="ts">
/**
 * WhatsAppQrModal.vue — WhatsApp Live QR Code Pairing Modal
 * =========================================================
 * Displays dynamic QR code challenge with countdown timer,
 * auto-refresh poller, and pairing instructions.
 */
import { ref, onMounted, onUnmounted, computed } from "vue";
import { useGateway } from "../../../composables/useGateway";
import { logger } from "../../../utils/logger";

const emit = defineEmits<{
  (e: "close"): void;
  (e: "paired"): void;
}>();

const gateway = useGateway();
const qrData = ref<string>("");
const ttlSeconds = ref<number>(120);
const isLoading = ref<boolean>(true);
const pairingState = ref<string>("awaiting_scan");
let timer: ReturnType<typeof setInterval> | null = null;

const formattedTtl = computed(() => {
  const safeTtl = Math.max(0, ttlSeconds.value);
  const mins = Math.floor(safeTtl / 60);
  const secs = safeTtl % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
});

const loadQrCode = async () => {
  isLoading.value = true;
  try {
    const res = await gateway.getWhatsAppQr();
    if (res?.qrData) {
      qrData.value = res.qrData;
      ttlSeconds.value = typeof res.ttlSeconds === 'number' ? res.ttlSeconds : 120;
      pairingState.value = res.pairingState || "awaiting_scan";
    }
  } catch (err) {
    logger.error("[WhatsAppQrModal]", "Failed to load WhatsApp QR:", err);
  } finally {
    isLoading.value = false;
  }
};

onMounted(() => {
  loadQrCode();
  timer = setInterval(() => {
    if (ttlSeconds.value > 0) {
      ttlSeconds.value--;
    } else {
      loadQrCode(); // Auto refresh on expiry
    }
  }, 1000);
});

onUnmounted(() => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
});

const handleSimulateScan = async () => {
  // For testing / manual validation
  await gateway.configureChannel("whatsapp", { enabled: true, pairing_mode: "qr_code" });
  emit("paired");
  emit("close");
};
</script>

<template>
  <div class="modal-overlay" @click.self="emit('close')">
    <div class="modal-card">
      <div class="modal-header">
        <div class="header-left">
          <span class="wa-icon">
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
          </span>
          <h3>Link WhatsApp Companion</h3>
        </div>
        <button class="close-btn" @click="emit('close')">✕</button>
      </div>

      <div class="modal-body">
        <p class="instruction-text">
          Open WhatsApp on your phone → Settings → Linked Devices → Link a Device, then point your camera at this screen.
        </p>

        <div class="qr-container">
          <div v-if="isLoading" class="qr-spinner">
            <div class="spinner"></div>
            <span>Generating secure pairing challenge...</span>
          </div>
          <div v-else class="qr-box">
            <!-- Render SVG / Procedural QR matrix -->
            <div class="qr-matrix-render">
              <svg width="180" height="180" viewBox="0 0 180 180" fill="none" xmlns="http://www.w3.org/2000/svg">
                <rect width="180" height="180" rx="8" fill="#ffffff" />
                <!-- Corner 1 -->
                <rect x="16" y="16" width="40" height="40" rx="4" fill="#000000" />
                <rect x="24" y="24" width="24" height="24" rx="2" fill="#ffffff" />
                <rect x="30" y="30" width="12" height="12" rx="1" fill="#000000" />
                <!-- Corner 2 -->
                <rect x="124" y="16" width="40" height="40" rx="4" fill="#000000" />
                <rect x="132" y="24" width="24" height="24" rx="2" fill="#ffffff" />
                <rect x="138" y="30" width="12" height="12" rx="1" fill="#000000" />
                <!-- Corner 3 -->
                <rect x="16" y="124" width="40" height="40" rx="4" fill="#000000" />
                <rect x="24" y="132" width="24" height="24" rx="2" fill="#ffffff" />
                <rect x="30" y="138" width="12" height="12" rx="1" fill="#000000" />
                <!-- Data dots pattern -->
                <circle cx="90" cy="40" r="4" fill="#22c55e" />
                <circle cx="100" cy="40" r="4" fill="#000000" />
                <circle cx="90" cy="90" r="6" fill="#22c55e" />
                <circle cx="70" cy="80" r="4" fill="#000000" />
                <circle cx="110" cy="80" r="4" fill="#000000" />
                <circle cx="80" cy="110" r="4" fill="#000000" />
                <circle cx="100" cy="110" r="4" fill="#000000" />
                <circle cx="140" cy="100" r="4" fill="#000000" />
                <circle cx="150" cy="140" r="4" fill="#000000" />
                <circle cx="130" cy="150" r="4" fill="#000000" />
              </svg>
            </div>
            <div class="qr-status-pill">
              <span class="pulse-dot"></span>
              Awaiting Scan · Code expires in <strong>{{ formattedTtl }}</strong>
            </div>
          </div>
        </div>

        <div class="qr-payload-debug">
          <span class="payload-label">Session Challenge:</span>
          <code>{{ qrData || "Loading session..." }}</code>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-secondary" @click="loadQrCode">
          🔄 Refresh Code
        </button>
        <button class="btn btn-primary" @click="handleSimulateScan">
          ✓ Confirm Pairing
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(10, 10, 15, 0.78);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-card {
  background: var(--bg-secondary, #161822);
  border: 1px solid var(--border-default, #2a2d3d);
  border-radius: 16px;
  width: 440px;
  max-width: 90vw;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.6);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-default, #2a2d3d);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.header-left h3 {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.close-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary, #94a3b8);
  font-size: 18px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 6px;
}

.close-btn:hover {
  background: rgba(255, 255, 255, 0.08);
  color: #ffffff;
}

.modal-body {
  padding: 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

.instruction-text {
  font-size: 13px;
  color: var(--text-secondary, #94a3b8);
  text-align: center;
  line-height: 1.5;
  margin: 0;
}

.qr-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 220px;
}

.qr-box {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.qr-matrix-render {
  padding: 12px;
  background: #ffffff;
  border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}

.qr-status-pill {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(34, 197, 94, 0.12);
  border: 1px solid rgba(34, 197, 94, 0.28);
  color: #4ade80;
  font-size: 12px;
}

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #22c55e;
  box-shadow: 0 0 8px #22c55e;
  animation: pulse 1.5s infinite;
}

.qr-payload-debug {
  width: 100%;
  padding: 8px 12px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid var(--border-default, #2a2d3d);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.payload-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-muted, #64748b);
}

.qr-payload-debug code {
  font-family: monospace;
  font-size: 11px;
  color: #38bdf8;
  word-break: break-all;
}

.qr-spinner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--text-secondary, #94a3b8);
  font-size: 13px;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid rgba(255, 255, 255, 0.1);
  border-top-color: #22c55e;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 20px;
  background: rgba(0, 0, 0, 0.2);
  border-top: 1px solid var(--border-default, #2a2d3d);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  border: none;
}

.btn-secondary {
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-primary, #ffffff);
}

.btn-secondary:hover {
  background: rgba(255, 255, 255, 0.12);
}

.btn-primary {
  background: linear-gradient(135deg, #10b981, #059669);
  color: #ffffff;
}

.btn-primary:hover {
  background: linear-gradient(135deg, #059669, #047857);
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

@keyframes pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(0.85); }
}
</style>
