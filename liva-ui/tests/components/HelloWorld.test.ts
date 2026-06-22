import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import HelloWorld from "../../src/components/HelloWorld.vue";

vi.mock("../assets/vite.svg", () => ({ default: "vite.svg" }));
vi.mock("../assets/hero.png", () => ({ default: "hero.png" }));
vi.mock("../assets/vue.svg", () => ({ default: "vue.svg" }));
vi.mock("/icons.svg", () => ({ default: "icons.svg" }));

describe("HelloWorld.vue", () => {
  it("should mount and count up", async () => {
    const wrapper = mount(HelloWorld);
    expect(wrapper.exists()).toBe(true);
    expect(wrapper.text()).toContain("Get started");
    const button = wrapper.find("button.counter");
    expect(button.text()).toContain("Count is 0");
    await button.trigger("click");
    expect(button.text()).toContain("Count is 1");
  });
});
