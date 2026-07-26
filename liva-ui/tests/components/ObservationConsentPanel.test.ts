/**
 * Cổng đồng ý quan sát thụ động (U20 bước 1) — phía giao diện.
 *
 * Vì sao có riêng file này: lõi đã kiểm 9/9 qua WebSocket thật, nhưng người
 * dùng không gõ WebSocket. Nghiệm thu U20 đòi cổng **hoạt động**, tức phải có
 * nút bấm được và nút đó phải gửi đúng lệnh. Đây là phần kiểm điều đó, và nó
 * chạy được trong CI (không cần model, không cần tiến trình sống).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { ref } from "vue";

const observationConsent = ref({ granted: false, active: false, updatedAt: null as number | null });
const sendMsg = vi.fn();

const gatewayMock = {
  observationConsent,
  sendMsg,
  isConnected: ref(true),
};

vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => gatewayMock,
}));

vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => `translated_${key}`,
    currentLang: ref("vi-VN"),
  }),
}));

import ObservationConsentPanel from "../../src/components/dashboard/ObservationConsentPanel.vue";

describe("ObservationConsentPanel — cổng đồng ý U20", () => {
  beforeEach(() => {
    sendMsg.mockClear();
    observationConsent.value = { granted: false, active: false, updatedAt: null };
  });

  it("hỏi trạng thái thật khi mở, không giả định gì", () => {
    mount(ObservationConsentPanel);
    expect(sendMsg).toHaveBeenCalledWith("consent:get");
  });

  it("mặc định hiện ĐANG TẮT — fail-closed cả ở giao diện", () => {
    const w = mount(ObservationConsentPanel);
    expect(w.text()).toContain("ĐANG TẮT");
    expect(w.text()).not.toContain("ĐÃ CHO PHÉP");
  });

  it("nút bật gửi đúng lệnh consent:grant", async () => {
    const w = mount(ObservationConsentPanel);
    sendMsg.mockClear();
    await w.find("button").trigger("click");
    expect(sendMsg).toHaveBeenCalledWith("consent:grant");
  });

  it("khi đã bật thì nút đổi thành thu hồi, gửi consent:revoke", async () => {
    observationConsent.value = { granted: true, active: false, updatedAt: 1_700_000_000 };
    const w = mount(ObservationConsentPanel);
    expect(w.text()).toContain("ĐÃ CHO PHÉP");
    sendMsg.mockClear();
    await w.find("button").trigger("click");
    expect(sendMsg).toHaveBeenCalledWith("consent:revoke");
  });

  /**
   * Ranh giới quan trọng nhất của toàn bộ U20 bước 1: **đã cho phép ≠ đang ghi**.
   * Chưa có collector nên dù bật cổng, giao diện vẫn phải nói rõ không có gì
   * đang được ghi. Nhầm hai khái niệm này là cách nhanh nhất để một tính năng
   * riêng tư mất lòng tin.
   */
  it("bật cổng KHÔNG được hiện 'đang ghi' khi chưa có collector", () => {
    observationConsent.value = { granted: true, active: false, updatedAt: 1_700_000_000 };
    const w = mount(ObservationConsentPanel);
    expect(w.text()).toContain("Không có gì đang được ghi");
    expect(w.find(".dang-ghi.on").exists()).toBe(false);
  });

  it("nói rõ chức năng thu thập CHƯA tồn tại", () => {
    const w = mount(ObservationConsentPanel);
    expect(w.text()).toContain("CHƯA tồn tại");
  });

  it("chỉ báo đỏ chỉ sáng khi lõi báo active", () => {
    observationConsent.value = { granted: true, active: true, updatedAt: 1 };
    const w = mount(ObservationConsentPanel);
    expect(w.find(".dang-ghi.on").exists()).toBe(true);
    expect(w.text()).toContain("Đang ghi");
  });
});
