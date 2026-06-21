import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { execute, metadata } from "../../../src/skills/devops/GitHubOperator";

// Hoist mock references
const { mockSafeFetch, mockRequestApproval } = vi.hoisted(() => ({
  mockSafeFetch: vi.fn(),
  mockRequestApproval: vi.fn().mockResolvedValue(true)
}));
(globalThis as any).mockSafeFetch = mockSafeFetch;
(globalThis as any).mockRequestApproval = mockRequestApproval;

vi.mock("@utils/HttpClient", () => ({
  safeFetch: (...args: any[]) => (globalThis as any).mockSafeFetch?.(...args)
}));

vi.mock("@security/HITLGuard", () => ({
  HITLGuard: {
    requestApproval: (...args: any[]) => (globalThis as any).mockRequestApproval?.(...args)
  }
}));

vi.mock("@utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn()
  }
}));

describe("Skill - GitHubOperator", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it("should export metadata", () => {
    expect(metadata.name).toBe("github_operator");
    expect(metadata.kit).toBe("DEVOPS_KIT");
  });

  describe("Token & Param Validation", () => {
    it("should return error if GITHUB_TOKEN is not defined and mock mode is disabled", async () => {
      // Force real mode but remove token
      process.env.NODE_ENV = "production";
      process.env.LIVA_MOCK_GITHUB = "false";
      delete process.env.GITHUB_TOKEN;

      const result = await execute({ action: "get_repo", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB ERROR] GITHUB_TOKEN environment variable is not defined");
    });

    it("should return error for invalid repository format", async () => {
      const result = await execute({ action: "get_repo", repo: "invalid_repo" });
      expect(result).toContain("[GITHUB ERROR] Invalid repository format");
    });

    it("should return error if title is missing for create_issue", async () => {
      const result = await execute({ action: "create_issue", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB ERROR] Title is required to create an issue");
    });

    it("should return error if title, head, or base is missing for create_pull_request", async () => {
      const result = await execute({ action: "create_pull_request", repo: "octocat/Hello-World", title: "PR" });
      expect(result).toContain("[GITHUB ERROR] Title, head, and base are required to create a pull request");
    });
  });

  describe("Mock Mode Execution", () => {
    beforeEach(() => {
      process.env.NODE_ENV = "test"; // triggers isMock = true
    });

    it("should successfully return mock repo details", async () => {
      const result = await execute({ action: "get_repo", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Repository: octocat/Hello-World");
      expect(result).toContain("Description: Mock repository description");
      expect(result).toContain("Stars: 42");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should successfully return mock issues list", async () => {
      const result = await execute({ action: "get_issues", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("#1: Mock Issue 1 (Status: open)");
      expect(result).toContain("#2: Mock Issue 2 (Status: closed)");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should successfully return mock pull requests list", async () => {
      const result = await execute({ action: "get_pull_requests", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("#1: Mock Pull Request 1 (Status: open)");
      expect(result).toContain("#2: Mock PR 2 (Status: open)");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should create mock issue after HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);

      const result = await execute({
        action: "create_issue",
        repo: "octocat/Hello-World",
        title: "Test issue title",
        body: "Test body"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Successfully created issue #101");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should return block message if HITL is rejected for create_issue", async () => {
      mockRequestApproval.mockRejectedValueOnce(new Error("REJECTED_BY_USER"));

      const result = await execute({
        action: "create_issue",
        repo: "octocat/Hello-World",
        title: "Test issue title",
        body: "Test body"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[GITHUB ACTION BLOCKED]");
      expect(result).toContain("rejected by user: REJECTED_BY_USER");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should create mock pull request after HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);

      const result = await execute({
        action: "create_pull_request",
        repo: "octocat/Hello-World",
        title: "New feature",
        body: "PR body",
        head: "feature-branch",
        base: "main"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Successfully created pull request #202");
      expect(mockSafeFetch).not.toHaveBeenCalled();
    });
  });

  describe("Real API Mode Execution (using safeFetch)", () => {
    beforeEach(() => {
      process.env.NODE_ENV = "production";
      process.env.GITHUB_TOKEN = "test-token";
      process.env.LIVA_MOCK_GITHUB = "false";
    });

    it("should successfully retrieve repo details via safeFetch", async () => {
      const mockResponse = {
        full_name: "octocat/Hello-World",
        description: "My first repo",
        stargazers_count: 100,
        forks_count: 10
      };
      mockSafeFetch.mockResolvedValueOnce({
        status: 200,
        ok: true,
        json: async () => mockResponse
      });

      const result = await execute({ action: "get_repo", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Repository: octocat/Hello-World");
      expect(result).toContain("Stars: 100");
      expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });

    it("should successfully retrieve issues list via safeFetch", async () => {
      const mockResponse = [
        { number: 1, title: "Bug 1", state: "open" },
        { number: 2, title: "Bug 2", state: "closed" }
      ];
      mockSafeFetch.mockResolvedValueOnce({
        status: 200,
        ok: true,
        json: async () => mockResponse
      });

      const result = await execute({ action: "get_issues", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("#1: Bug 1 (Status: open)");
      expect(result).toContain("#2: Bug 2 (Status: closed)");
      expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });

    it("should successfully retrieve pull requests list via safeFetch", async () => {
      const mockResponse = [
        { number: 10, title: "Feature 10", state: "open" }
      ];
      mockSafeFetch.mockResolvedValueOnce({
        status: 200,
        ok: true,
        json: async () => mockResponse
      });

      const result = await execute({ action: "get_pull_requests", repo: "octocat/Hello-World" });
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("#10: Feature 10 (Status: open)");
      expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });

    it("should create issue via safeFetch after HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);
      const mockResponse = {
        number: 42,
        title: "Test issue title",
        html_url: "https://github.com/octocat/Hello-World/issues/42"
      };
      mockSafeFetch.mockResolvedValueOnce({
        status: 201,
        ok: true,
        json: async () => mockResponse
      });

      const result = await execute({
        action: "create_issue",
        repo: "octocat/Hello-World",
        title: "Test issue title",
        body: "Test body"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Successfully created issue #42");
      expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });

    it("should create pull request via safeFetch after HITL approval", async () => {
      mockRequestApproval.mockResolvedValueOnce(true);
      const mockResponse = {
        number: 100,
        title: "New feature",
        html_url: "https://github.com/octocat/Hello-World/pull/100"
      };
      mockSafeFetch.mockResolvedValueOnce({
        status: 201,
        ok: true,
        json: async () => mockResponse
      });

      const result = await execute({
        action: "create_pull_request",
        repo: "octocat/Hello-World",
        title: "New feature",
        body: "PR body",
        head: "feature-branch",
        base: "main"
      });

      expect(mockRequestApproval).toHaveBeenCalledTimes(1);
      expect(result).toContain("[GITHUB SUCCESS]");
      expect(result).toContain("Successfully created pull request #100");
      expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });
  });
});
