import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";
import ObsidianVaultView from "../../src/components/dashboard/ObsidianVaultView.vue";

// Mock useGateway
const isConnectedRef = ref(true);
vi.mock("../../src/composables/useGateway", () => ({
  useGateway: () => ({
    isConnected: isConnectedRef,
    init: vi.fn(),
    sendMsg: vi.fn(),
  }),
}));

// Mock useI18n
vi.mock("../../src/composables/useI18n", () => ({
  useI18n: () => ({
    t: (key: string) => (key === "nav_vault" ? "Obsidian PKM Knowledge Vault" : key),
    currentLang: ref("en-US"),
  }),
}));

// Mock useToast
const mockToast = {
  show: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  warn: vi.fn(),
  info: vi.fn(),
  dismiss: vi.fn(),
  clear: vi.fn(),
  toasts: ref([]),
};
vi.mock("../../src/composables/useToast", () => ({
  useToast: () => mockToast,
}));

describe("ObsidianVaultView.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should render vault banner, notes explorer list, and markdown reader", () => {
    const wrapper = mount(ObsidianVaultView);

    expect(wrapper.text()).toContain("Obsidian PKM Knowledge Vault");
    expect(wrapper.text()).toContain("teamwork_projects/obsidian_llm_wiki/vault/");
    expect(wrapper.text()).toContain("LIVA Unified Native Architecture");
    expect(wrapper.find(".reader-pane").exists()).toBe(true);
    expect(wrapper.text()).toContain("YAML FRONTMATTER");
  });

  it("should filter notes by category", async () => {
    const wrapper = mount(ObsidianVaultView);

    const categoryChips = wrapper.findAll(".chip-btn");
    const aiCoreChip = categoryChips.find(c => c.text() === "AI Core");
    expect(aiCoreChip).toBeDefined();

    await aiCoreChip!.trigger("click");
    await wrapper.vm.$nextTick();

    const noteCards = wrapper.findAll(".note-item-card");
    expect(noteCards.length).toBe(2);
    expect(wrapper.text()).toContain("Hybrid RAG");
    expect(wrapper.text()).toContain("Swarm DAG StateGraph");
  });

  it("should filter notes by search query", async () => {
    const wrapper = mount(ObsidianVaultView);

    const searchInput = wrapper.find<HTMLInputElement>(".search-box input");
    await searchInput.setValue("DPAPI");
    await wrapper.vm.$nextTick();

    const noteCards = wrapper.findAll(".note-item-card");
    expect(noteCards.length).toBe(1);
    expect(wrapper.text()).toContain("Security Auditing");
  });

  it("should switch selected note when clicking a note item", async () => {
    const wrapper = mount(ObsidianVaultView);

    const noteCards = wrapper.findAll(".note-item-card");
    await noteCards[1].trigger("click");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".reader-pane").text()).toContain("Hybrid RAG: Vector Embeddings & FTS5 Fusion");
  });

  it("should navigate when clicking wikilink pill", async () => {
    const wrapper = mount(ObsidianVaultView);

    const wikilinks = wrapper.findAll(".wikilink-pill");
    const ragLink = wikilinks.find(l => l.text().includes("hybrid-rag-vector-search"));
    expect(ragLink).toBeDefined();

    await ragLink!.trigger("click");
    expect(mockToast.info).toHaveBeenCalledWith(expect.stringContaining("Navigated to"));
  });

  it("should copy markdown content to clipboard", async () => {
    const wrapper = mount(ObsidianVaultView);

    // Mock clipboard
    Object.assign(navigator, {
      clipboard: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    });

    const copyBtn = wrapper.findAll("button").find(b => b.text().includes("Copy"));
    expect(copyBtn).toBeDefined();

    await copyBtn!.trigger("click");
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
    expect(mockToast.success).toHaveBeenCalledWith("Markdown copied to clipboard!");
  });

  it("should create new note in vault", async () => {
    const wrapper = mount(ObsidianVaultView);

    const newNoteBtn = wrapper.findAll("button").find(b => b.text().includes("New Note"));
    expect(newNoteBtn).toBeDefined();

    await newNoteBtn!.trigger("click");
    expect(mockToast.success).toHaveBeenCalledWith("Created new draft note in vault.");
    expect(wrapper.find(".reader-pane").text()).toContain("Untitled Note");
  });

  it("should trigger vault sync", async () => {
    const wrapper = mount(ObsidianVaultView);

    const syncBtn = wrapper.findAll("button").find(b => b.text().includes("Sync Vault"));
    expect(syncBtn).toBeDefined();

    await syncBtn!.trigger("click");
    vi.advanceTimersByTime(500);
    await flushPromises();

    expect(mockToast.success).toHaveBeenCalledWith("Obsidian Vault indexed successfully (53 notes, 142 backlinks).");
  });
});
