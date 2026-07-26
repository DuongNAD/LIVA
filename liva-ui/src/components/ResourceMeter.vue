<script setup lang="ts">
/**
 * ResourceMeter.vue — dải hiện CHI PHÍ tài nguyên của LIVA (U16).
 *
 * ## Vì sao dải này tồn tại
 *
 * Mọi demo trợ lý AI đều giấu cái giá tài nguyên: người xem thấy trợ lý trả
 * lời, không thấy nó ăn bao nhiêu máy. LIVA là dự án hiếm hoi có đủ số liệu để
 * **hiện** con số đó — `governor.rs` đo tải ngoài LIVA và trừ đúng phần của
 * chính mình, `sysinfo.rs` từ chối đoán khi không đo được.
 *
 * Một đồng hồ gần như đứng yên trong lúc trợ lý đang nói là bằng chứng **không
 * dựng được bằng cắt ghép**, và nó chứng minh đúng trụ cột khó tin nhất:
 * "sống chung với tải nặng".
 *
 * ## Hai số phải đứng CẠNH nhau mới có nghĩa
 *
 * `cpuUsage` một mình chỉ nói "máy đang bận" — không chứng minh LIVA rẻ.
 * `livaCpuUsage` một mình không có gì để so. Cặp "máy 92 % · LIVA 3 %" mới là
 * câu chuyện, và hai số dùng chung mẫu số (xem `governor::own_cpu_percent`) nên
 * đọc cạnh nhau là hợp lệ.
 *
 * ## `--` là một câu trả lời, không phải lỗi
 *
 * Lõi trả `null` khi thật sự không đo được: máy không có NVIDIA, không phải
 * Windows, hoặc lần lấy mẫu đầu chưa có mốc để so. Ở đây hiện `--` chứ **không**
 * lấp bằng 0 — một ô trống nói thật có ích hơn một con số đẹp nói dối. Nếu dải
 * này từng hiện `0 %` trong khi máy đang tải nặng thì nó đã tự phá giá trị duy
 * nhất của chính mình.
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useGateway } from "../composables/useGateway";

const gateway = useGateway();

/** Nhịp poll. 2 s đủ nhanh để thấy phản ứng khi mở tải nặng, đủ chậm để bản
 *  thân việc đo không thành một khoản chi phí đáng kể. */
const NHIP_MS = 2000;

let hen: ReturnType<typeof setInterval> | null = null;
const daNhanSo = ref(false);

/**
 * ⚠️ KHÔNG gate theo `gateway.isConnected` — nó `false` ở **cả hai** profile:
 *
 * - Vỏ Tauri: `connect()` cố ý return sớm vì lệnh đi qua `invoke`, nên cờ này
 *   không bao giờ bật dù `sendMsg` chạy tốt.
 * - Widget trong trình duyệt: `WidgetApp` không gọi `gateway.init()` (nó tự mở
 *   một WS thoại riêng), nên composable chưa từng kết nối.
 *
 * Bản đầu của dải này gate theo `isConnected` và hệ quả là nó **không bao giờ
 * hiện** — không lỗi, không cảnh báo, chỉ đơn giản là trống. Đúng loại hỏng
 * lặng lẽ mà chính U16 sinh ra để chống.
 */
const doSo = () => gateway.sendMsg("get_system_status");

const os = computed(() => gateway.systemStatus.value?.osStats);

/** `null`/`undefined` → `--`. KHÔNG bao giờ quy về 0. */
const phanTram = (v: number | null | undefined) =>
  typeof v === "number" ? `${v}%` : "--";

const may = computed(() => phanTram(os.value?.cpuUsage));
const liva = computed(() => phanTram(os.value?.livaCpuUsage));
const gpu = computed(() => phanTram(os.value?.gpuUsage));

/**
 * Governor đang nhường máy (fullscreen, hoặc tải ngoài vượt ngưỡng).
 *
 * Đường dẫn là `healthChecks.vramGuard.isYielded` — KHÔNG phải cấp cao nhất.
 * Viết nhầm thành `systemStatus.isYielded` thì nó luôn `undefined`, tức cờ này
 * sẽ **không bao giờ sáng** mà cũng chẳng có lỗi nào để lần ra.
 */
const dangNhuong = computed(
  () => gateway.systemStatus.value?.healthChecks?.vramGuard?.isYielded === true,
);

onMounted(() => {
  // Tự đảm bảo có kết nối thay vì trông chờ vỏ chứa gọi hộ. `connect()` có
  // `if (ws.value) return` nên gọi lại là vô hại, và ở Tauri nó return sớm.
  gateway.init();
  doSo();
  hen = setInterval(doSo, NHIP_MS);
});
onUnmounted(() => {
  if (hen) clearInterval(hen);
  hen = null;
});

// Chỉ hiện khi đã nhận được ít nhất một đáp ứng — tránh nháy một dải toàn `--`
// lúc khởi động, dễ bị đọc nhầm thành "đo hỏng".
const hienThi = computed(() => {
  if (os.value !== undefined) daNhanSo.value = true;
  return daNhanSo.value;
});
</script>

<template>
  <div v-if="hienThi" class="resource-meter" :class="{ nhuong: dangNhuong }">
    <span class="o">
      <span class="nhan">Máy</span><span class="so">{{ may }}</span>
    </span>
    <span class="o nhan-manh">
      <span class="nhan">LIVA</span><span class="so">{{ liva }}</span>
    </span>
    <span class="o">
      <span class="nhan">GPU</span><span class="so">{{ gpu }}</span>
    </span>
    <span v-if="dangNhuong" class="co-nhuong" title="Governor đang hạ ưu tiên để nhường máy">⏬</span>
  </div>
</template>

<style scoped>
.resource-meter {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 3px 9px;
  border-radius: 999px;
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.14));
  background: var(--bg-tertiary, rgba(0, 0, 0, 0.45));
  font-size: 11px;
  line-height: 1.4;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  user-select: none;
}

/* Đang nhường máy là trạng thái ĐÁNG KHOE, không phải cảnh báo — tô nhẹ để
   người xem để ý đúng lúc governor làm việc. */
.resource-meter.nhuong {
  border-color: var(--accent, #6ea8fe);
}

.o {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}

.nhan {
  color: var(--text-secondary, rgba(255, 255, 255, 0.55));
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.so {
  color: var(--text-primary, #fff);
  font-weight: 600;
}

/* Con số của LIVA là thứ người xem cần nhìn — nó mới là "cái giá". */
.o.nhan-manh .so {
  color: var(--accent, #6ea8fe);
}

.co-nhuong {
  font-size: 10px;
}
</style>
