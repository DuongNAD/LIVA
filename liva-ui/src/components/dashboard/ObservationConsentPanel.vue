<script setup lang="ts">
/**
 * ObservationConsentPanel.vue — công tắc đồng ý quan sát thụ động (U20, bước 1).
 *
 * ## Vì sao panel này tồn tại TRƯỚC phần thu thập
 *
 * Nghiệm thu U20 đặt một thứ tự bắt buộc: cổng đồng ý và công tắc tắt phải
 * **tồn tại và hoạt động trước khi viết dòng code thu thập đầu tiên**. IPC lõi
 * (`consent:*`) đã kiểm chứng 9/9 qua socket thật — nhưng người dùng không gõ
 * WebSocket, nên "công tắc hoạt động" còn cần một nút bấm được. Đây là nút đó.
 *
 * ## Cố ý nói RÕ rằng chưa có gì bị ghi
 *
 * Đây là mặt phẳng quyền riêng tư của sản phẩm, và bối cảnh của nó là một ý
 * tưởng từng bị công chúng ném đá (bộ nhớ thị giác). Nên panel không được để mơ
 * hồ: nó nói thẳng "chức năng thu thập CHƯA tồn tại", để bật cổng bây giờ không
 * khiến ai tưởng máy mình đang bị ghi. Bật = *cho phép trước*, không phải *đang
 * chạy*.
 */
import { computed } from "vue";
import { useGateway } from "../../composables/useGateway";
import { useI18n } from "../../composables/useI18n";

const gateway = useGateway();
const { currentLang } = useI18n();
const vi = computed(() => currentLang.value === "vi-VN");

const consent = computed(() => gateway.observationConsent.value);

const doc = () => gateway.sendMsg("consent:get");
const bat = () => gateway.sendMsg("consent:grant");
const tat = () => gateway.sendMsg("consent:revoke");

// Đọc trạng thái thật ngay khi panel hiện — không giả định gì.
doc();

const thoiDiem = computed(() => {
  const t = consent.value.updatedAt;
  if (!t) return "";
  return new Date(t * 1000).toLocaleString(vi.value ? "vi-VN" : "en-US");
});
</script>

<template>
  <div class="consent-panel card">
    <div class="tieu-de">
      <span class="icon">🔒</span>
      <h3>{{ vi ? "Quan sát thụ động" : "Passive observation" }}</h3>
      <span class="trang-thai" :class="consent.granted ? 'bat' : 'tat'">
        {{ consent.granted ? (vi ? "ĐÃ CHO PHÉP" : "ALLOWED") : (vi ? "ĐANG TẮT" : "OFF") }}
      </span>
    </div>

    <p class="mo-ta">
      {{ vi
        ? "Cho phép LIVA quan sát ngữ cảnh màn hình (tên cửa sổ, tiến trình) để về sau nhớ được “hôm qua mình làm gì”. Mọi thứ nằm trên máy bạn, không rời đi đâu."
        : "Let LIVA observe screen context (window titles, processes) so it can later recall “what was I doing yesterday”. Everything stays on your machine." }}
    </p>

    <!-- Ranh giới quan trọng nhất của panel: đã cho phép ≠ đang ghi. -->
    <div class="chua-co-note">
      <strong>{{ vi ? "Chức năng thu thập CHƯA tồn tại." : "Collection does not exist yet." }}</strong>
      {{ vi
        ? "Bật công tắc này là cho phép TRƯỚC. Chưa có dòng code nào ghi lại gì — cổng đồng ý được làm xong trước phần thu thập một cách có chủ đích. Khi thu thập ra đời, nó bị chặn cứng nếu công tắc này tắt."
        : "Turning this on grants permission ahead of time. No code records anything yet — the consent gate is deliberately built before collection. When collection ships, it is hard-blocked whenever this switch is off." }}
    </div>

    <div class="hang-nut">
      <button v-if="!consent.granted" class="btn btn-primary" @click="bat">
        ✅ {{ vi ? "Cho phép quan sát" : "Allow observation" }}
      </button>
      <button v-else class="btn btn-danger" @click="tat">
        ⛔ {{ vi ? "Thu hồi cho phép" : "Revoke permission" }}
      </button>
      <span v-if="thoiDiem" class="thoi-diem">
        {{ vi ? "Cập nhật:" : "Updated:" }} {{ thoiDiem }}
      </span>
    </div>

    <!-- Chỉ báo "đang ghi": chưa có collector nên luôn tắt. Vẫn hiện để chỗ nối
         sẵn sàng và để người dùng thấy rõ hiện tại KHÔNG có gì đang chạy. -->
    <div class="dang-ghi" :class="{ on: consent.active }">
      <span class="cham" />
      {{ consent.active
        ? (vi ? "Đang ghi" : "Recording")
        : (vi ? "Không có gì đang được ghi" : "Nothing is being recorded") }}
    </div>
  </div>
</template>

<style scoped>
.consent-panel {
  padding: var(--space-lg, 20px);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.tieu-de {
  display: flex;
  align-items: center;
  gap: 10px;
}
.tieu-de h3 {
  margin: 0;
  font-size: 15px;
}
.trang-thai {
  margin-left: auto;
  padding: 2px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.04em;
  border: 1px solid var(--border-default);
}
.trang-thai.bat {
  color: var(--accent, #6ea8fe);
  border-color: var(--accent, #6ea8fe);
}
.trang-thai.tat {
  color: var(--text-secondary);
}
.mo-ta {
  margin: 0;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.6;
}
.chua-co-note {
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--border-default);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  font-size: 12.5px;
  line-height: 1.6;
}
.chua-co-note strong {
  color: var(--text-primary);
}
.hang-nut {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.thoi-diem {
  color: var(--text-secondary);
  font-size: 11px;
}
.dang-ghi {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--text-secondary);
}
.dang-ghi .cham {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-secondary);
  opacity: 0.4;
}
.dang-ghi.on .cham {
  background: #e0245e;
  opacity: 1;
}
</style>
