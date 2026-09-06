<script setup lang="ts">
/**
 * NodePairingView.vue — Companion Node & Device Pairing Monitor
 * ===============================================================
 * Monitor paired companion devices (mobile, tablet, desktop, server nodes),
 * approve/reject incoming 6-digit cryptographic short-code challenges,
 * revoke stale device access, and generate onboarding QR codes.
 */
import { ref, onMounted, computed } from "vue";
import { useGateway } from "../../composables/useGateway";
import { logger } from "../../utils/logger";
import type { PairedNodeInfo, PendingPairingChallenge } from "liva-common";

const gateway = useGateway();
const loading = ref(true);
const manualCode = ref("");
const approvingCode = ref<string | null>(null);
const revokingNodeId = ref<string | null>(null);
const creatingChallenge = ref(false);
const toastMessage = ref<string | null>(null);
const generatedChallenge = ref<{ shortCode: string; challengeId: string; qrPayload: string } | null>(null);

const showToast = (msg: string) => {
  toastMessage.value = msg;
  setTimeout(() => {
    toastMessage.value = null;
  }, 3500);
};

const pairedNodes = computed<PairedNodeInfo[]>(() => {
  return gateway.pairedNodesList.value || [];
});

const pendingChallenges = computed<PendingPairingChallenge[]>(() => {
  return gateway.pendingPairingList.value || [];
});

const refreshData = async () => {
  loading.value = true;
  try {
    await Promise.all([
      gateway.fetchPairedNodes(),
      gateway.fetchPendingPairing(),
    ]);
  } catch (err) {
    logger.error("[NodePairingView]", "Failed to fetch pairing data:", err);
  } finally {
    loading.value = false;
  }
};

const handleApproveCode = async (code: string) => {
  approvingCode.value = code;
  try {
    const res = await gateway.approvePairing({ shortCode: code });
    if (res?.paired) {
      showToast(`Device paired successfully! Short code ${code} approved.`);
      manualCode.value = "";
    }
  } catch (err) {
    showToast(`Approval failed: ${err}`);
  } finally {
    approvingCode.value = null;
  }
};

const handleRejectChallenge = async (challengeId: string) => {
  try {
    await gateway.rejectPairing(challengeId, "Rejected by administrator");
    showToast("Pairing challenge rejected.");
  } catch (err) {
    showToast(`Rejection failed: ${err}`);
  }
};

const handleRevokeNode = async (nodeId: string, nodeName: string) => {
  revokingNodeId.value = nodeId;
  try {
    await gateway.revokePairing(nodeId);
    showToast(`Revoked access for device "${nodeName}".`);
  } catch (err) {
    showToast(`Revocation failed: ${err}`);
  } finally {
    revokingNodeId.value = null;
  }
};

const handleCreateSampleChallenge = async () => {
  creatingChallenge.value = true;
  try {
    const res = await gateway.createPairingChallenge(
      "Mobile Companion Phone",
      "mobile_companion",
      "ed25519_pubkey_" + Math.random().toString(36).substring(2, 9)
    );
    if (res?.shortCode) {
      generatedChallenge.value = {
        shortCode: res.shortCode,
        challengeId: res.challengeId,
        qrPayload: res.qrPayload,
      };
      showToast(`Generated pairing code: ${res.shortCode}`);
    }
  } catch (err) {
    showToast(`Failed to generate pairing challenge: ${err}`);
  } finally {
    creatingChallenge.value = false;
  }
};

const formatTimeAgo = (unix: number) => {
  if (!unix) return "Never";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unix;
  if (diff < 60) return "Just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} mins ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} hours ago`;
  return `${Math.floor(diff / 86400)} days ago`;
};

const getDeviceIcon = (deviceType: string) => {
  switch (deviceType) {
    case "mobile":
      return "📱";
    case "desktop":
      return "💻";
    case "server":
      return "🖥️";
    case "terminal":
      return "⌨️";
    case "widget":
      return "🪟";
    default:
      return "📡";
  }
};

onMounted(() => {
  refreshData();
});
</script>

<template>
  <div class="pairing-view animate-fadeIn">
    <!-- Header -->
    <header class="view-header">
      <div class="header-titles">
        <h2>Node Pairing Monitor</h2>
        <p class="subtitle">
          Manage Zero-Trust companion devices, verify 6-digit short-code pairing challenges, and audit active cryptographic sessions.
        </p>
      </div>
      <div class="header-actions">
        <button class="btn btn-emerald" :disabled="creatingChallenge" @click="handleCreateSampleChallenge">
          <span>{{ creatingChallenge ? 'Generating...' : '➕ New Pairing Code' }}</span>
        </button>
        <button class="btn btn-secondary" @click="refreshData">
          🔄 Refresh
        </button>
      </div>
    </header>

    <!-- Toast Banner -->
    <Transition name="fade">
      <div v-if="toastMessage" class="toast-banner">
        <span>{{ toastMessage }}</span>
      </div>
    </Transition>

    <!-- Quick Approve Short-Code Form -->
    <div class="card approve-card">
      <div class="approve-header">
        <div class="approve-title">
          <span class="icon">🔑</span>
          <div>
            <h3>Quick Code Verification</h3>
            <span class="subtext">Enter the 6-digit code shown on your companion mobile app or terminal</span>
          </div>
        </div>
      </div>
      <div class="approve-form">
        <input
          v-model="manualCode"
          type="text"
          maxlength="6"
          placeholder="e.g. 849201"
          class="code-input"
          @keyup.enter="manualCode.length === 6 && handleApproveCode(manualCode)"
        />
        <button
          class="btn btn-primary"
          :disabled="manualCode.trim().length !== 6 || approvingCode !== null"
          @click="handleApproveCode(manualCode.trim())"
        >
          {{ approvingCode === manualCode ? 'Verifying...' : 'Approve & Connect' }}
        </button>
      </div>
    </div>

    <!-- Active Pairing Challenge Banner if just generated -->
    <div v-if="generatedChallenge" class="card generated-challenge-box">
      <div class="challenge-info">
        <span class="badge badge-reconnecting">Awaiting Companion Connection</span>
        <h4>6-Digit Pairing PIN: <strong class="big-pin">{{ generatedChallenge.shortCode }}</strong></h4>
        <p class="subtext">Scan this QR payload or enter this PIN on your companion client within 5 minutes.</p>
        <div class="qr-payload-box">
          <code>{{ generatedChallenge.qrPayload }}</code>
        </div>
      </div>
      <button class="btn-close" @click="generatedChallenge = null">✕</button>
    </div>

    <!-- Pending Challenges Section -->
    <div class="section-container">
      <div class="section-title-row">
        <h3>Pending Pairing Requests ({{ pendingChallenges.length }})</h3>
        <span class="section-hint">Ed25519 challenges waiting for administrator confirmation</span>
      </div>

      <div v-if="pendingChallenges.length === 0" class="empty-state-card">
        <span class="empty-icon">🛡️</span>
        <p>No pending pairing challenges. All companion devices are verified or idle.</p>
      </div>

      <div v-else class="challenges-grid">
        <div v-for="ch in pendingChallenges" :key="ch.challengeId" class="card pending-card">
          <div class="pending-top">
            <div class="node-info">
              <span class="device-icon">📱</span>
              <div>
                <h4>{{ ch.nodeName }}</h4>
                <span class="node-role-badge">{{ ch.role }}</span>
              </div>
            </div>
            <div class="pin-badge">
              <span>PIN: </span>
              <strong>{{ ch.shortCode }}</strong>
            </div>
          </div>

          <div class="pending-meta">
            <div class="meta-row">
              <span class="label">Public Key:</span>
              <span class="val mono">{{ ch.publicKey.substring(0, 16) }}...</span>
            </div>
            <div class="meta-row">
              <span class="label">Expires in:</span>
              <span class="val">{{ ch.ttlRemainingSeconds }}s</span>
            </div>
          </div>

          <div class="pending-actions">
            <button
              class="btn btn-secondary btn-sm"
              @click="handleRejectChallenge(ch.challengeId)"
            >
              Reject
            </button>
            <button
              class="btn btn-primary btn-sm"
              :disabled="approvingCode === ch.shortCode"
              @click="handleApproveCode(ch.shortCode)"
            >
              {{ approvingCode === ch.shortCode ? 'Pairing...' : '✓ Approve' }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Approved Paired Devices Section -->
    <div class="section-container">
      <div class="section-title-row">
        <h3>Active Companion Nodes ({{ pairedNodes.length }})</h3>
        <span class="section-hint">Authorized client devices with active session tokens</span>
      </div>

      <div v-if="pairedNodes.length === 0" class="empty-state-card">
        <span class="empty-icon">🔌</span>
        <p>No paired devices currently connected. Generate a pairing code above to connect your mobile app or CLI.</p>
      </div>

      <div v-else class="nodes-table-card">
        <table class="nodes-table">
          <thead>
            <tr>
              <th>Device</th>
              <th>Role</th>
              <th>Public Key Fingerprint</th>
              <th>Last Heartbeat</th>
              <th>Paired Since</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="node in pairedNodes" :key="node.nodeId">
              <td>
                <div class="device-cell">
                  <span class="dev-icon">{{ getDeviceIcon(node.deviceType) }}</span>
                  <div>
                    <strong>{{ node.nodeName }}</strong>
                    <span class="device-type-label">{{ node.deviceType }}</span>
                  </div>
                </div>
              </td>
              <td>
                <span class="role-pill">{{ node.role }}</span>
              </td>
              <td>
                <code class="pubkey-code">{{ node.publicKey ? node.publicKey.substring(0, 18) + '...' : 'ed25519_key' }}</code>
              </td>
              <td>
                <span class="heartbeat-active">🟢 {{ formatTimeAgo(node.lastSeenUnix) }}</span>
              </td>
              <td>
                <span class="time-label">{{ formatTimeAgo(node.approvedAtUnix) }}</span>
              </td>
              <td>
                <button
                  class="btn btn-danger btn-sm"
                  :disabled="revokingNodeId === node.nodeId"
                  @click="handleRevokeNode(node.nodeId, node.nodeName)"
                  title="Revoke session token and disconnect"
                >
                  {{ revokingNodeId === node.nodeId ? 'Revoking...' : 'Revoke' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pairing-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 24px;
  overflow-y: auto;
  gap: 24px;
  background: var(--bg-secondary, #0e1017);
}

.view-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.header-titles h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary, #ffffff);
  margin: 0 0 6px 0;
}

.subtitle {
  font-size: 13px;
  color: var(--text-secondary, #94a3b8);
  margin: 0;
  max-width: 650px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.toast-banner {
  padding: 10px 16px;
  border-radius: 8px;
  background: rgba(56, 189, 248, 0.12);
  border: 1px solid rgba(56, 189, 248, 0.3);
  color: #38bdf8;
  font-size: 13px;
  font-weight: 500;
}

.card {
  background: rgba(18, 20, 29, 0.7);
  border: 1px solid var(--border-default, #242738);
  border-radius: 12px;
  backdrop-filter: blur(10px);
}

.approve-card {
  padding: 18px 20px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
  flex-wrap: wrap;
}

.approve-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.approve-title .icon {
  font-size: 24px;
}

.approve-title h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0 0 2px 0;
}

.subtext {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
}

.approve-form {
  display: flex;
  align-items: center;
  gap: 12px;
}

.code-input {
  width: 140px;
  letter-spacing: 4px;
  font-size: 16px;
  font-weight: 700;
  text-align: center;
  background: #080a10;
  border: 1px solid var(--border-default, #242738);
  border-radius: 8px;
  padding: 8px 12px;
  color: #38bdf8;
}

.code-input:focus {
  outline: none;
  border-color: #38bdf8;
  box-shadow: 0 0 10px rgba(56, 189, 248, 0.2);
}

.generated-challenge-box {
  padding: 16px 20px;
  background: rgba(56, 189, 248, 0.05);
  border-color: rgba(56, 189, 248, 0.3);
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.challenge-info {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.big-pin {
  color: #38bdf8;
  font-size: 18px;
  letter-spacing: 2px;
  margin-left: 6px;
}

.qr-payload-box code {
  background: #080a10;
  padding: 4px 10px;
  border-radius: 4px;
  font-size: 11px;
  color: #94a3b8;
  font-family: monospace;
}

.section-container {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.section-title-row {
  display: flex;
  align-items: baseline;
  gap: 12px;
}

.section-title-row h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.section-hint {
  font-size: 12px;
  color: var(--text-muted, #64748b);
}

.empty-state-card {
  padding: 30px;
  border-radius: 12px;
  border: 1px dashed var(--border-default, #242738);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-muted, #64748b);
  font-size: 13px;
}

.empty-icon {
  font-size: 32px;
  opacity: 0.5;
}

.challenges-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
}

.pending-card {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.pending-top {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.node-info {
  display: flex;
  align-items: center;
  gap: 10px;
}

.device-icon {
  font-size: 24px;
}

.node-info h4 {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary, #ffffff);
  margin: 0;
}

.node-role-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.08);
  color: #94a3b8;
}

.pin-badge {
  background: rgba(56, 189, 248, 0.15);
  border: 1px solid rgba(56, 189, 248, 0.3);
  padding: 4px 8px;
  border-radius: 6px;
  font-size: 12px;
  color: #38bdf8;
}

.pending-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-secondary, #94a3b8);
}

.meta-row {
  display: flex;
  justify-content: space-between;
}

.pending-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 8px;
  border-top: 1px solid rgba(255, 255, 255, 0.04);
}

.nodes-table-card {
  overflow-x: auto;
}

.nodes-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: 13px;
}

.nodes-table th {
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.02);
  color: var(--text-secondary, #94a3b8);
  font-weight: 600;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border-default, #242738);
}

.nodes-table td {
  padding: 14px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  color: var(--text-primary, #ffffff);
}

.device-cell {
  display: flex;
  align-items: center;
  gap: 10px;
}

.dev-icon {
  font-size: 20px;
}

.device-type-label {
  display: block;
  font-size: 11px;
  color: var(--text-muted, #64748b);
}

.role-pill {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 4px;
  background: rgba(129, 140, 248, 0.12);
  color: #a5b4fc;
}

.pubkey-code {
  font-family: monospace;
  font-size: 11px;
  color: #94a3b8;
  background: rgba(0, 0, 0, 0.3);
  padding: 2px 6px;
  border-radius: 4px;
}

.heartbeat-active {
  font-size: 12px;
  color: #4ade80;
}

.time-label {
  font-size: 12px;
  color: var(--text-muted, #64748b);
}

.btn-emerald {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.3);
  padding: 8px 14px;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
}
.btn-emerald:hover {
  background: rgba(16, 185, 129, 0.25);
  border-color: #34d399;
}
</style>
