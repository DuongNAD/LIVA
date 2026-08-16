import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import SkeletonLoader from "../../src/components/SkeletonLoader.vue";

describe("SkeletonLoader.vue", () => {
  it("should render default text skeleton with single line", () => {
    const wrapper = mount(SkeletonLoader);
    expect(wrapper.classes()).toContain("skeleton-type-text");
    expect(wrapper.classes()).toContain("skeleton-animated");
    expect(wrapper.findAll(".skeleton-line").length).toBe(1);
  });

  it("should render multiple text lines when lines prop is passed", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "text",
        lines: 3,
      },
    });
    expect(wrapper.findAll(".skeleton-line").length).toBe(3);
  });

  it("should render avatar skeleton", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "avatar",
        count: 2,
      },
    });
    expect(wrapper.classes()).toContain("skeleton-type-avatar");
    expect(wrapper.findAll(".skeleton-avatar").length).toBe(2);
  });

  it("should render card skeleton with header and body", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "card",
        count: 1,
      },
    });
    expect(wrapper.find(".skeleton-card").exists()).toBe(true);
    expect(wrapper.find(".skeleton-card-header").exists()).toBe(true);
    expect(wrapper.find(".skeleton-card-body").exists()).toBe(true);
  });

  it("should render list skeleton", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "list",
        count: 3,
      },
    });
    expect(wrapper.findAll(".skeleton-list-item").length).toBe(3);
  });

  it("should render table row skeleton", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "table",
        count: 2,
      },
    });
    expect(wrapper.findAll(".skeleton-table-row").length).toBe(2);
  });

  it("should support custom width, height, and non-animated mode", () => {
    const wrapper = mount(SkeletonLoader, {
      props: {
        type: "rect",
        width: "200px",
        height: "80px",
        borderRadius: "16px",
        animated: false,
      },
    });

    expect(wrapper.classes()).not.toContain("skeleton-animated");
    const rect = wrapper.find(".skeleton-rect");
    expect(rect.attributes("style")).toContain("width: 200px;");
    expect(rect.attributes("style")).toContain("height: 80px;");
    expect(rect.attributes("style")).toContain("border-radius: 16px;");
  });
});
