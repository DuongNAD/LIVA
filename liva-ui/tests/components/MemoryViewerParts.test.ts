import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import MemoryViewerHeader from "../../src/components/dashboard/memory/MemoryViewerHeader.vue";
import MemoryViewerStats from "../../src/components/dashboard/memory/MemoryViewerStats.vue";
import MemoryViewerTabs from "../../src/components/dashboard/memory/MemoryViewerTabs.vue";

describe("MemoryViewer extracted components", () => {
  it("emits header actions without owning gateway state", async () => {
    const wrapper = mount(MemoryViewerHeader, {
      props: {
        currentLang: "vi-VN",
        totalMemories: 12,
        isConsolidating: false,
        isRefreshing: false,
        isRestarting: false,
        isRestartArmed: false,
        recentMemories: 2,
        restartError: "",
      },
    });
    const buttons = wrapper.findAll("button");
    await buttons[0].trigger("click");
    await buttons[1].trigger("click");
    await buttons[2].trigger("click");
    expect(wrapper.emitted("consolidate")).toHaveLength(1);
    expect(wrapper.emitted("refresh")).toHaveLength(1);
    expect(wrapper.emitted("restart")).toHaveLength(1);
    expect(wrapper.text()).toContain("LIVA vừa nhớ thêm 2 điều");
  });

  it("selects all five memory layers from the stats cards", async () => {
    const wrapper = mount(MemoryViewerStats, {
      props: {
        activeTab: "facts",
        currentLang: "en-US",
        l0Count: 1,
        l05Size: "2 B",
        l05NotWired: true,
        factsCount: 3,
        eventsCount: 4,
        vectorsCount: 5,
      },
    });
    for (const card of wrapper.findAll(".stat-card")) {
      await card.trigger("click");
    }
    expect(wrapper.emitted("update:activeTab")?.map(([value]) => value)).toEqual([
      "l0",
      "l0_5",
      "facts",
      "events",
      "vectors",
    ]);
  });

  it("selects all five memory layers from tab navigation", async () => {
    const wrapper = mount(MemoryViewerTabs, {
      props: { activeTab: "facts", currentLang: "vi-VN" },
    });
    for (const button of wrapper.findAll("button")) {
      await button.trigger("click");
    }
    expect(wrapper.emitted("update:activeTab")?.map(([value]) => value)).toEqual([
      "l0",
      "l0_5",
      "vectors",
      "events",
      "facts",
    ]);
  });
});
