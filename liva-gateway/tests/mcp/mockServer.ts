import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const server = new Server(
  {
    name: "mock-mcp-server",
    version: "1.0.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: "echo",
        description: "Echoes back the message",
        inputSchema: {
          type: "object",
          properties: {
            message: { type: "string" },
          },
          required: ["message"],
        },
      },
    ],
  };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  if (request.params.name === "echo") {
    const message = request.params.arguments?.message;
    return {
      content: [
        {
          type: "text",
          text: `Echo: ${message}`,
        },
      ],
    };
  }
  throw new Error(`Tool not found: ${request.params.name}`);
});

const transport = new StdioServerTransport();
await server.connect(transport);

// Explicitly handle stdin closing to prevent hanging processes
process.stdin.on("close", () => {
  process.exit(0);
});
process.stdin.on("end", () => {
  process.exit(0);
});
