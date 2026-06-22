import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { safeFetch } from "@utils/HttpClient";

const GitHubOperatorSchema = z.object({
  action: z.enum(["get_issues", "create_issue", "get_pull_requests", "create_pull_request", "get_repo"]),
  repo: z.string(),
  title: z.string().optional(),
  body: z.string().optional(),
  head: z.string().optional(),
  base: z.string().optional(),
});

export const metadata = {
  name: "github_operator",
  search_keywords: ["github", "pull request", "issue", "repository", "git", "mã nguồn"],
  description: "[ASK_FIRST] Interact with GitHub API to manage issues, pull requests, and retrieve repository details. Requires GITHUB_TOKEN.",
  kit: "DEVOPS_KIT",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["get_issues", "create_issue", "get_pull_requests", "create_pull_request", "get_repo"],
        description: "GitHub action to perform."
      },
      repo: {
        type: "string",
        description: "Repository name in owner/repo format (e.g., 'octocat/Hello-World')."
      },
      title: {
        type: "string",
        description: "Title for the issue or pull request (required for create_issue and create_pull_request)."
      },
      body: {
        type: "string",
        description: "Body/description for the issue or pull request (optional)."
      },
      head: {
        type: "string",
        description: "The name of the branch where your changes are implemented (required for create_pull_request)."
      },
      base: {
        type: "string",
        description: "The name of the branch you want your changes pulled into (required for create_pull_request)."
      }
    },
    required: ["action", "repo"]
  }
};

export const execute = async (argsObj: unknown): Promise<string> => {
  try {
    const parsed = GitHubOperatorSchema.parse(argsObj);
    const { action, repo, title, body, head, base } = parsed;

    if (!repo.includes("/")) {
      return `[GITHUB ERROR] Invalid repository format. Must be 'owner/repo' (e.g., 'octocat/Hello-World').`;
    }

    if (action === "create_issue" && !title) {
      return `[GITHUB ERROR] Title is required to create an issue.`;
    }

    if (action === "create_pull_request" && (!title || !head || !base)) {
      return `[GITHUB ERROR] Title, head, and base are required to create a pull request.`;
    }

    const isMock = process.env.LIVA_MOCK_GITHUB === 'true' ||
                   (process.env.LIVA_MOCK_GITHUB !== 'false' && (!process.env.GITHUB_TOKEN || process.env.NODE_ENV === 'test'));

    const writeActions = ["create_issue", "create_pull_request"];
    if (writeActions.includes(action)) {
      logger.info(`[GitHubOperator] Action '${action}' requires HITL approval...`);
      try {
        await HITLGuard.requestApproval({
          toolName: "github_operator",
          args: { action, repo, title, body, head, base },
          reason: `LIVA wants to perform a write action on GitHub repository ${repo}: ${action}`
        });
        logger.info(`[GitHubOperator] ✅ HITL Approved for action: ${action}`);
      } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.warn(`[GitHubOperator] ❌ HITL Rejected: ${errMsg}`);
        return `[GITHUB ACTION BLOCKED] Action '${action}' on repository '${repo}' was rejected by user: ${errMsg}`;
      }
    }

    if (isMock) {
      let resultSummary = "";
      if (action === "get_repo") {
        resultSummary = `Repository: ${repo}\nDescription: Mock repository description\nStars: 42\nForks: 7`;
      } else if (action === "get_issues") {
        resultSummary = `#1: Mock Issue 1 (Status: open)\n#2: Mock Issue 2 (Status: closed)`;
      } else if (action === "get_pull_requests") {
        resultSummary = `#1: Mock Pull Request 1 (Status: open)\n#2: Mock PR 2 (Status: open)`;
      } else if (action === "create_issue") {
        resultSummary = `Successfully created issue #101: "${title}"\nURL: https://github.com/${repo}/issues/101`;
      } else if (action === "create_pull_request") {
        resultSummary = `Successfully created pull request #202: "${title}"\nURL: https://github.com/${repo}/pulls/202`;
      }
      return `[GITHUB SUCCESS] Action: ${action}\n\n[OUTPUT]\n${resultSummary}`;
    }

    const token = process.env.GITHUB_TOKEN;
    if (!token) {
      return `[GITHUB ERROR] GITHUB_TOKEN environment variable is not defined. Please configure it in your environment.`;
    }

    const headers: Record<string, string> = {
      "Authorization": `token ${token}`,
      "Accept": "application/vnd.github.v3+json",
      "User-Agent": "LIVA-Gateway"
    };

    interface GitHubIssue {
      number: number;
      title: string;
      state: string;
    }

    interface GitHubPR {
      number: number;
      title: string;
      state: string;
    }

    let url = "";
    let method = "GET";
    let payload: Record<string, unknown> | null = null;

    if (action === "get_repo") {
      url = `https://api.github.com/repos/${repo}`;
    } else if (action === "get_issues") {
      url = `https://api.github.com/repos/${repo}/issues`;
    } else if (action === "get_pull_requests") {
      url = `https://api.github.com/repos/${repo}/pulls`;
    } else if (action === "create_issue") {
      url = `https://api.github.com/repos/${repo}/issues`;
      method = "POST";
      payload = { title, body: body || "" };
    } else if (action === "create_pull_request") {
      url = `https://api.github.com/repos/${repo}/pulls`;
      method = "POST";
      payload = { title, body: body || "", head, base };
    }

    const options: RequestInit = {
      method,
      headers
    };

    if (payload) {
      options.body = JSON.stringify(payload);
      headers["Content-Type"] = "application/json";
    }

    const response = await safeFetch(url, options);
    const data = await response.json();

    let resultSummary = "";
    if (action === "get_repo") {
      resultSummary = `Repository: ${data.full_name}\nDescription: ${data.description || "No description"}\nStars: ${data.stargazers_count}\nForks: ${data.forks_count}`;
    } else if (action === "get_issues") {
      if (Array.isArray(data)) {
        resultSummary = (data as GitHubIssue[]).slice(0, 10).map((issue) => `#${issue.number}: ${issue.title} (Status: ${issue.state})`).join("\n") || "No issues found.";
      } else {
        resultSummary = "Unexpected format for issues list.";
      }
    } else if (action === "get_pull_requests") {
      if (Array.isArray(data)) {
        resultSummary = (data as GitHubPR[]).slice(0, 10).map((pr) => `#${pr.number}: ${pr.title} (Status: ${pr.state})`).join("\n") || "No pull requests found.";
      } else {
        resultSummary = "Unexpected format for pull requests list.";
      }
    } else if (action === "create_issue") {
      resultSummary = `Successfully created issue #${data.number}: "${data.title}"\nURL: ${data.html_url}`;
    } else if (action === "create_pull_request") {
      resultSummary = `Successfully created pull request #${data.number}: "${data.title}"\nURL: ${data.html_url}`;
    }

    return `[GITHUB SUCCESS] Action: ${action}\n\n[OUTPUT]\n${resultSummary}`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[GitHubOperator] Error: ${errMsg}`);
    if (error instanceof z.ZodError) {
      return `[GITHUB ERROR] Parameter validation failed: ${error.issues.map(e => e.message).join(", ")}`;
    }
    return `[GITHUB ERROR] Failed to execute action: ${errMsg}`;
  }
};
