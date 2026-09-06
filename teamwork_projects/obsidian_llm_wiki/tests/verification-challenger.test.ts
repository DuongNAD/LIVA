import { vi } from 'vitest';
import type { Client as McpClient } from '@modelcontextprotocol/sdk/client/index.js';
import type { InMemoryTransport as McpInMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';

// Initialize a global state object for symlink mocking
(globalThis as any).symlinkMockState = {
  enabled: false,
  selfLoopPath: '',
  loopAPath: '',
  loopBPath: '',
};

// Mock the 'fs' module while preserving every real export except the two
// symlink probes needed by the circular-link verification cases.
vi.mock('fs', async (importOriginal) => {
  const orig = await importOriginal<typeof import('fs')>();
  const path = await import('path');
  return {
    __esModule: true,
    ...orig,
    lstatSync: (p: any) => {
      const state = (globalThis as any).symlinkMockState;
      if (state && state.enabled) {
        const resolvedP = path.resolve(p);
        if (state.selfLoopPath && resolvedP.toLowerCase() === state.selfLoopPath.toLowerCase()) {
          return {
            isSymbolicLink: () => true,
            isFile: () => false,
            isDirectory: () => false,
          } as any;
        }
        if (state.loopAPath && resolvedP.toLowerCase() === state.loopAPath.toLowerCase()) {
          return {
            isSymbolicLink: () => true,
            isFile: () => false,
            isDirectory: () => false,
          } as any;
        }
        if (state.loopBPath && resolvedP.toLowerCase() === state.loopBPath.toLowerCase()) {
          return {
            isSymbolicLink: () => true,
            isFile: () => false,
            isDirectory: () => false,
          } as any;
        }
      }
      return orig.lstatSync(p);
    },
    readlinkSync: (p: any) => {
      const state = (globalThis as any).symlinkMockState;
      if (state && state.enabled) {
        const resolvedP = path.resolve(p);
        if (state.selfLoopPath && resolvedP.toLowerCase() === state.selfLoopPath.toLowerCase()) {
          return 'self_loop.md';
        }
        if (state.loopAPath && resolvedP.toLowerCase() === state.loopAPath.toLowerCase()) {
          return 'loop_b.md';
        }
        if (state.loopBPath && resolvedP.toLowerCase() === state.loopBPath.toLowerCase()) {
          return 'loop_a.md';
        }
      }
      return orig.readlinkSync(p);
    }
  };
});

// Import dynamically after mock is registered
const { promises: fsp } = await import('fs');
const fs = await import('fs');
const path = await import('path');
const os = await import('os');
const { fileURLToPath } = await import('url');
const { InMemoryTransport } = await import('@modelcontextprotocol/sdk/inMemory.js');
const { Client } = await import('@modelcontextprotocol/sdk/client/index.js');
const { createMcpServer } = await import('../src/server.js');
const { validateAndResolvePath } = await import('../src/vault.js');

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface ToolTextContent {
  type: 'text';
  text: string;
}

interface CallToolSuccessResult {
  content: ToolTextContent[];
  isError?: boolean;
}

// Hàng rào chống treo cho các lệnh gọi tool trong bộ test tính đúng đắn này.
//
// KHÔNG phải phép đo hiệu năng. Bản trước từng có `expect(duration)
// .toBeLessThan(1000)` và nó NHẤP NHÁY: đỏ trên runner macOS ở 1521 ms vì
// runner GitHub dùng shared machine chậm hơn máy dev một cách hợp lệ — ngưỡng
// đó đo TỐC ĐỘ MÁY chứ không đo mã nguồn. Chế độ hỏng mà tên test muốn bắt là
// treo VÔ HẠN, mà treo vô hạn thì bất kỳ hạn nào cũng bắt được, nên hàng rào
// được nới RỘNG có chủ đích để không đánh đổi độ tin cậy lấy thứ không cần.
// Một cổng CI đỏ ngẫu nhiên thì tệ hơn không có cổng, vì nó dạy người ta bấm
// "chạy lại". Cùng lập luận với doc-comment của
// `speaker_queue_day_fail_fast_khong_giu_blocking_thread`
// (liva-native-core/src/webrtc/pipeline.rs).
const HANG_FENCE_MS = 30_000;

async function callToolWithHangFence(callPromise: Promise<unknown>, label: string): Promise<unknown> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      callPromise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} TREO quá ${HANG_FENCE_MS}ms — hàng rào chống treo bắn`)),
          HANG_FENCE_MS
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

describe('LIVA-Obsidian MCP Server Verification Challenger Tests', () => {
  let tempVaultPath: string;
  let client: McpClient;
  let mcpServer: any;
  let clientTransport: McpInMemoryTransport;
  let serverTransport: McpInMemoryTransport;
  let cleanupServer: () => void;

  beforeEach(async () => {
    // Create a unique temporary directory for the mock vault
    tempVaultPath = await fsp.mkdtemp(path.join(os.tmpdir(), 'mcp-vault-verification-'));
    
    // Copy sample vault contents to the temp vault directory
    const sourceVault = path.resolve(__dirname, '../vault');
    await fsp.cp(sourceVault, tempVaultPath, { recursive: true });

    // Instantiate MCP server configured with the temp vault path
    const serverResult = createMcpServer(tempVaultPath);
    mcpServer = serverResult.mcpServer;
    cleanupServer = serverResult.cleanup;

    // Create linked transports for in-memory communication
    [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();

    // Connect the server to its transport
    await mcpServer.connect(serverTransport);

    // Connect the client to its transport
    client = new Client(
      {
        name: 'verification-client',
        version: '1.0.0'
      },
      {
        capabilities: {}
      }
    );
    await client.connect(clientTransport);
  });

  afterEach(async () => {
    if (client) {
      await client.close();
    }
    if (mcpServer) {
      await mcpServer.close();
    }
    if (cleanupServer) {
      cleanupServer();
    }
    await fsp.rm(tempVaultPath, { recursive: true, force: true });
    
    // Disable any symlink mocks
    (globalThis as any).symlinkMockState.enabled = false;
  });

  describe('1. Path Traversal & UNC Inputs', () => {
    test('URL encoded path traversal attempts are safely rejected', async () => {
      const complexTraversals = [
        '%2e%2e%2foutside.md',                // ../outside.md
        '%2e%2e/outside.md',                  // ../outside.md
        '..%2foutside.md',                    // ../outside.md
        'Knowledge/%2e%2e/%2e%2e/outside.md',  // Knowledge/../../outside.md
        'Knowledge/%2e%2e%2f%2e%2e%2foutside.md',
        'Knowledge/..%2f..%2foutside.md',
        '..%5c..%5coutside.md',                // ..\..\outside.md
        '%252e%252e%252foutside.md',          // double encoded: %2e%2e/outside.md
      ];

      for (const badPath of complexTraversals) {
        const rawResult = await client.callTool({
          name: 'read_markdown',
          arguments: { path: badPath }
        });
        const readResult = rawResult as unknown as CallToolSuccessResult;
        expect(readResult.isError).toBe(true);
        expect(readResult.content[0].text).toMatch(/Access denied|File not found/);
      }
    });

    test('UNC paths and Windows absolute paths are safely rejected', async () => {
      const uncPaths = [
        '\\\\localhost\\c$\\Windows\\win.ini',
        '\\\\?\\c:\\Windows\\win.ini',
        '\\\\.\\PhysicalDrive0',
        '//localhost/c$/Windows/win.ini',
        'c:\\Windows\\win.ini',
        'd:/somefile.md',
      ];

      for (const badPath of uncPaths) {
        const rawResult = await client.callTool({
          name: 'read_markdown',
          arguments: { path: badPath }
        });
        const readResult = rawResult as unknown as CallToolSuccessResult;
        expect(readResult.isError).toBe(true);
        // VC-3b: trên POSIX, `\` là ký tự tên file HỢP LỆ nên chuỗi UNC được giữ
        // nguyên BÊN TRONG vault ⇒ đi qua kiểm bao hàm rồi dừng ở "file not
        // found" — hành vi ĐÚNG của macOS/Linux, chỉ khác thông điệp Windows.
        // Khẳng định phải biết nền thay vì đòi thông điệp của nền kia.
        if (process.platform === 'win32') {
          expect(readResult.content[0].text).toContain('Access denied');
        } else {
          expect(readResult.content[0].text).toMatch(/Access denied|File not found/);
        }
      }
    });

    test('Paths containing null bytes or control characters are rejected', async () => {
      const badPaths = [
        'Knowledge/file.md\0',
        'Knowledge/file.md\x00',
        'Knowledge/file\u0000.md',
        'Knowledge/file\x01.md',
        'Knowledge/file\x7F.md',
      ];

      for (const badPath of badPaths) {
        const rawResult = await client.callTool({
          name: 'read_markdown',
          arguments: { path: badPath }
        });
        const readResult = rawResult as unknown as CallToolSuccessResult;
        expect(readResult.isError).toBe(true);
        expect(readResult.content[0].text).toContain('Access denied');
      }
    });

    test('Circular symlinks are detected and rejected to prevent infinite loops', () => {
      (globalThis as any).symlinkMockState = {
        enabled: true,
        // ⚠️ Khoá mock bằng canonicalised root: trên macOS `/var` LÀ một symlink
        // tới `/private/var`, còn `validateAndResolvePath` canonicalise vault
        // root bằng `fs.realpathSync` trước khi ghép path. Khoá theo
        // `tempVaultPath` thô thì hai chuỗi không bao giờ khớp ⇒ mock không bắn
        // ⇒ test đỏ trên macOS (VC-3a).
        selfLoopPath: path.resolve(fs.realpathSync(tempVaultPath), 'Skills/self_loop.md'),
      };

      try {
        expect(() => {
          validateAndResolvePath(tempVaultPath, 'Skills/self_loop.md');
        }).toThrow(/Symlink loop detected/i);
      } finally {
        (globalThis as any).symlinkMockState.enabled = false;
      }
    });

    test('Double symlink loops are detected and rejected', () => {
      (globalThis as any).symlinkMockState = {
        enabled: true,
        // Cùng lý do canonicalised root với test phía trên (VC-3a).
        loopAPath: path.resolve(fs.realpathSync(tempVaultPath), 'Skills/loop_a.md'),
        loopBPath: path.resolve(fs.realpathSync(tempVaultPath), 'Skills/loop_b.md'),
      };

      try {
        expect(() => {
          validateAndResolvePath(tempVaultPath, 'Skills/loop_a.md');
        }).toThrow(/Symlink loop detected/i);
      } finally {
        (globalThis as any).symlinkMockState.enabled = false;
      }
    });
  });

  describe('2. Large Files & Malformed/Corrupt File Content', () => {
    test('Server handles thousands of lines (large files) gracefully', async () => {
      const lineCount = 10000;
      const largeContentLines = [
        '---',
        'title: "Large File Test"',
        'tags:',
        '  - test/large',
        'author: "Challenger"',
        '---',
        '# Large File',
      ];
      for (let i = 0; i < lineCount; i++) {
        largeContentLines.push(`This is line number ${i} of our extremely large test document.`);
      }
      largeContentLines.push('TargetKeywordForSearch at the very end of the file.');
      const largeContent = largeContentLines.join('\n');

      const filePath = 'Knowledge/large_file.md';
      
      const rawWrite = await client.callTool({
        name: 'write_markdown',
        arguments: { path: filePath, content: largeContent }
      });
      expect((rawWrite as any).isError).toBeFalsy();

      const rawRead = await client.callTool({
        name: 'read_markdown',
        arguments: { path: filePath }
      });
      const readResult = rawRead as unknown as CallToolSuccessResult;
      expect(readResult.content[0].text.length).toBe(largeContent.length);

      // Hàng rào chống treo (HANG_FENCE_MS) — KHÔNG phải phép đo hiệu năng.
      // Bản trước là `expect(searchTime).toBeLessThan(1000)` đo tốc độ máy,
      // không đo mã nguồn; cùng lớp lỗi với test "extremely long queries"
      // phía dưới. Xem comment ở hằng HANG_FENCE_MS.
      const rawSearch = await callToolWithHangFence(
        client.callTool({
          name: 'search_vault',
          arguments: { query: 'TargetKeywordForSearch' }
        }),
        'search_vault trên file 10 000 dòng',
      );

      const searchResult = rawSearch as unknown as CallToolSuccessResult;
      const results = JSON.parse(searchResult.content[0].text);
      expect(results.length).toBeGreaterThan(0);
      expect(results[0].path).toBe('Knowledge/large_file.md');
      expect(results[0].matches[0].lineNumber).toBe(10008);
    }, 60_000); // Timeout test PHẢI lớn hơn HANG_FENCE_MS để hàng rào là thứ báo đỏ khi treo thật.

    test('Server handles empty files safely', async () => {
      const filePath = 'Knowledge/empty_file.md';
      
      const rawWrite = await client.callTool({
        name: 'write_markdown',
        arguments: { path: filePath, content: '' }
      });
      expect((rawWrite as any).isError).toBeFalsy();

      const rawRead = await client.callTool({
        name: 'read_markdown',
        arguments: { path: filePath }
      });
      const readResult = rawRead as unknown as CallToolSuccessResult;
      expect(readResult.content[0].text).toBe('');
    });

    test('Server handles corrupt or malformed YAML frontmatter gracefully', async () => {
      const malformedFiles = [
        {
          path: 'Knowledge/malformed_frontmatter_1.md',
          content: `---
title: "Unclosed Frontmatter
author: "Test"
This frontmatter is never closed with three dashes.
# Content starts here
`
        },
        {
          path: 'Knowledge/malformed_frontmatter_2.md',
          content: `---
title: "Mismatched quotes'
author: "Test"
---
# Content starts here
`
        },
        {
          path: 'Knowledge/malformed_frontmatter_3.md',
          content: `---
title: "Duplicate keys"
title: "Another title"
---
# Content starts here
`
        }
      ];

      for (const file of malformedFiles) {
        const rawWrite = await client.callTool({
          name: 'write_markdown',
          arguments: { path: file.path, content: file.content }
        });
        expect((rawWrite as any).isError).toBeFalsy();

        const rawRead = await client.callTool({
          name: 'read_markdown',
          arguments: { path: file.path }
        });
        const readResult = rawRead as unknown as CallToolSuccessResult;
        expect(readResult.content[0].text).toBe(file.content);

        const rawSearch = await client.callTool({
          name: 'search_vault',
          arguments: { query: 'Content starts here' }
        });
        const searchResult = rawSearch as unknown as CallToolSuccessResult;
        const results = JSON.parse(searchResult.content[0].text);
        expect(results.length).toBeGreaterThan(0);
      }
    });
  });

  describe('3. Long & Special Character Search Queries', () => {
    test('Server handles extremely long queries without crashing or hanging', async () => {
      const extremelyLongQuery = 'a'.repeat(20000);
      // Hàng rào chống treo (HANG_FENCE_MS) — KHÔNG phải phép đo hiệu năng.
      // Bản trước là `expect(duration).toBeLessThan(1000)` và nhấp nháy trên
      // runner macOS: đỏ ở 1521 ms cho truy vấn 20 000 ký tự, tức runner
      // shared chỉ chậm hơn — không phải treo. Chế độ hỏng cần bắt là treo VÔ
      // HẠN nên hạn nào cũng bắt được; hàng rào được nới rộng có chủ đích.
      // Xem comment ở hằng HANG_FENCE_MS và doc-comment của
      // `speaker_queue_day_fail_fast_khong_giu_blocking_thread`
      // (liva-native-core/src/webrtc/pipeline.rs).
      const rawResult = await callToolWithHangFence(
        client.callTool({
          name: 'search_vault',
          arguments: { query: extremelyLongQuery }
        }),
        'search_vault với truy vấn 20 000 ký tự',
      );
      const result = rawResult as unknown as CallToolSuccessResult;

      expect(result.isError).toBeFalsy();
      const results = JSON.parse(result.content[0].text);
      expect(results.length).toBe(0);
    }, 60_000); // Timeout test PHẢI lớn hơn HANG_FENCE_MS để hàng rào là thứ báo đỏ khi treo thật.

    test('Server handles special characters in search query safely', async () => {
      const specialQueries = [
        '"',
        '""',
        '\'',
        'author:"explorer',
        'tags:liva/knowledge title:',
        'title:.*+?^${}()|[]\\',
        '🚀 漢 字 UTF-8 Emoji',
        '\' OR 1=1 --',
        '<script>alert("xss")</script>',
        'tags:a:b:c',
        '   whitespace   padding   ',
        '\\0',
      ];

      for (const query of specialQueries) {
        const rawResult = await client.callTool({
          name: 'search_vault',
          arguments: { query }
        });
        const result = rawResult as unknown as CallToolSuccessResult;
        expect(result.isError).toBeFalsy();
        const results = JSON.parse(result.content[0].text);
        expect(Array.isArray(results)).toBe(true);
      }
    });
  });
});
