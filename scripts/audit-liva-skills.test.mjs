/**
 * Kiểm thử `scripts/audit-liva-skills.mjs`.
 *
 * Chạy: node --test scripts/audit-liva-skills.test.mjs
 *
 * Mỗi phép kiểm dựng một cây thư mục tạm rồi soi kết quả, không đụng vào repo
 * thật — bộ kiểm phải bắt được lỗi, nên phần lớn ca ở đây là ca ÂM TÍNH
 * (front-matter hỏng, name lệch thư mục, liên kết chết). Một bộ kiểm chỉ có ca
 * dương tính không chứng minh được gì (`vault/Knowledge/anti_patterns.md`,
 * mục "False Green").
 */

import { after, before, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

import {
  auditWorkspace,
  extractRelativeLinks,
  parseArgs,
  parseFrontmatter,
} from './audit-liva-skills.mjs';

const execFileAsync = promisify(execFile);
const SCRIPT = fileURLToPath(new URL('./audit-liva-skills.mjs', import.meta.url));

let workspaces = [];

/** Dựng một cây tệp tạm từ bản đồ `đường-dẫn-tương-đối -> nội dung`. */
async function makeWorkspace(files) {
  const root = await mkdtemp(path.join(tmpdir(), 'liva-skill-audit-'));
  workspaces.push(root);
  await Promise.all([
    mkdir(path.join(root, '.claude/skills'), { recursive: true }),
    mkdir(path.join(root, '.agents/skills'), { recursive: true }),
    mkdir(path.join(root, 'teamwork_projects/obsidian_llm_wiki/vault/Skills'), {
      recursive: true,
    }),
    mkdir(path.join(root, 'teamwork_projects/obsidian_llm_wiki/vault/Knowledge'), {
      recursive: true,
    }),
    mkdir(path.join(root, 'teamwork_projects/obsidian_llm_wiki/vault/Rules'), {
      recursive: true,
    }),
  ]);
  for (const [relPath, content] of Object.entries(files)) {
    const abs = path.join(root, relPath);
    await mkdir(path.dirname(abs), { recursive: true });
    await writeFile(abs, content, 'utf8');
  }
  return root;
}

const codes = (findings) => findings.map((f) => f.code);

const goodSkill = (name) => `---
name: ${name}
description: "Dùng khi cần kiểm thử bộ audit."
---

# ${name}

Nội dung tối thiểu.
`;

const goodVaultNote = (title) => `---
title: "${title}"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-07-28T00:00:00Z"
---

# ${title}
`;

after(async () => {
  await Promise.all(workspaces.map((dir) => rm(dir, { recursive: true, force: true })));
  workspaces = [];
});

describe('parseFrontmatter', () => {
  it('đọc được scalar, danh sách gạch đầu dòng và mảng inline', () => {
    const parsed = parseFrontmatter(`---
title: "coding_standards"
tags:
  - liva/rule
  - liva/knowledge
inline: ["a", "b"]
---
body`);
    assert.equal(parsed.ok, true);
    assert.equal(parsed.data.title, 'coding_standards');
    assert.deepEqual(parsed.data.tags, ['liva/rule', 'liva/knowledge']);
    assert.deepEqual(parsed.data.inline, ['a', 'b']);
  });

  it('đọc được danh sách bản ghi thụt lề (kiểu `inputs:` của vault/Skills)', () => {
    const parsed = parseFrontmatter(`---
title: "web_search"
inputs:
  - name: "query"
    type: "string"
    description: "chuỗi tìm kiếm"
---
body`);
    assert.equal(parsed.ok, true);
    assert.deepEqual(parsed.data.inputs, [
      { name: 'query', type: 'string', description: 'chuỗi tìm kiếm' },
    ]);
  });

  it('báo hỏng khi thiếu `---` mở đầu', () => {
    const parsed = parseFrontmatter('# Không có front-matter\n');
    assert.equal(parsed.ok, false);
    assert.match(parsed.reason, /mở đầu/);
  });

  it('báo hỏng khi front-matter không được đóng lại', () => {
    const parsed = parseFrontmatter('---\nname: x\n\n# thân bài\n');
    assert.equal(parsed.ok, false);
    assert.match(parsed.reason, /đóng lại/);
  });

  it('báo hỏng khi khoá bị lặp', () => {
    const parsed = parseFrontmatter('---\nname: a\nname: b\n---\n');
    assert.equal(parsed.ok, false);
    assert.match(parsed.reason, /lặp lại/);
  });

  it('báo hỏng với dòng rác ở cột 0', () => {
    const parsed = parseFrontmatter('---\nname: a\nrác không có dấu hai chấm\n---\n');
    assert.equal(parsed.ok, false);
    assert.equal(parsed.line, 3);
  });
});

describe('extractRelativeLinks', () => {
  it('chỉ lấy liên kết tương đối, bỏ qua giao thức và neo', () => {
    const links = extractRelativeLinks(
      '[a](./a.md) [b](https://x.dev) [c](#muc) [d](mailto:x@y.z) [e](../e.md#muc)',
    );
    assert.deepEqual(
      links.map((l) => l.target),
      ['./a.md', '../e.md'],
    );
  });

  it('giải mã %20 trong tên file có dấu cách', () => {
    const [link] = extractRelativeLinks('[x](./Code%20Review.md)');
    assert.equal(link.target, './Code Review.md');
  });
});

describe('parseArgs', () => {
  it('nhận --json và cả hai dạng --root', () => {
    assert.equal(parseArgs(['--json']).json, true);
    assert.equal(parseArgs(['--root', '/tmp/x']).root, '/tmp/x');
    assert.equal(parseArgs(['--root=/tmp/y']).root, '/tmp/y');
  });
});

describe('auditWorkspace — cây sạch', () => {
  it('không lỗi, không cảnh báo', async () => {
    const root = await makeWorkspace({
      '.claude/skills/alpha/SKILL.md': goodSkill('alpha'),
      '.agents/skills/alpha/SKILL.md': goodSkill('alpha'),
      'teamwork_projects/obsidian_llm_wiki/vault/Rules/luat.md': goodVaultNote('luat'),
    });
    const result = await auditWorkspace(root);
    assert.deepEqual(result.errors, []);
    assert.deepEqual(result.warnings, []);
    assert.equal(result.scanned.skills, 2);
    assert.equal(result.scanned.vault, 1);
  });

  it('bỏ qua vault/Templates (file mẫu dùng placeholder `{{title}}`)', async () => {
    const root = await makeWorkspace({
      'teamwork_projects/obsidian_llm_wiki/vault/Templates/Skill Template.md':
        '---\ntitle: "{{title}}"\n---\n',
      'teamwork_projects/obsidian_llm_wiki/vault/Rules/luat.md': goodVaultNote('luat'),
    });
    const result = await auditWorkspace(root);
    assert.equal(result.scanned.vault, 1);
    assert.deepEqual(result.errors, []);
  });
});

describe('auditWorkspace — ca âm tính là LỖI', () => {
  it('placeholder scaffold trong SKILL.md là lỗi', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': `---
name: beta
description: [TODO: mô tả trigger]
---

## Structuring This Skill

[TODO: hoàn thiện nội dung]
`,
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['template-placeholder']);
  });

  it('thiếu thư mục gốc là lỗi thay vì cây rỗng báo xanh', async () => {
    const root = await makeWorkspace({});
    await rm(path.join(root, '.agents/skills'), { recursive: true, force: true });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['missing-root']);
    assert.equal(errors[0].file, '.agents/skills');
  });

  it('front-matter hỏng trong SKILL.md', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': '---\nname: beta\n\n# quên đóng front-matter\n',
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['bad-frontmatter']);
    assert.equal(errors[0].file, '.claude/skills/beta/SKILL.md');
  });

  it('`name` lệch tên thư mục', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': goodSkill('gamma'),
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['name-folder-mismatch']);
    assert.match(errors[0].message, /gamma/);
  });

  it('SKILL.md thiếu `description`', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': '---\nname: beta\n---\n\n# beta\n',
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['missing-key']);
  });

  it('ghi chép vault thiếu khoá bắt buộc', async () => {
    const root = await makeWorkspace({
      'teamwork_projects/obsidian_llm_wiki/vault/Knowledge/thieu.md':
        '---\ntitle: "thieu"\ntags:\n  - liva/knowledge\n---\n',
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['missing-key', 'missing-key']);
    assert.match(errors.map((e) => e.message).join(' '), /author/);
    assert.match(errors.map((e) => e.message).join(' '), /last_update/);
  });

  it('liên kết markdown tương đối trỏ vào file không tồn tại', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': `${goodSkill('beta')}\n[đi đâu](./references/khong-co.md)\n`,
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(codes(errors), ['broken-link']);
    assert.match(errors[0].message, /khong-co\.md/);
  });

  it('liên kết tương đối tồn tại thì KHÔNG báo lỗi', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': `${goodSkill('beta')}\n[có thật](./references/co.md)\n`,
      '.claude/skills/beta/references/co.md': '# có thật\n',
    });
    const { errors } = await auditWorkspace(root);
    assert.deepEqual(errors, []);
  });
});

describe('auditWorkspace — ca CẢNH BÁO không làm đỏ cổng', () => {
  it('không kích hoạt dấu vết ngoại lai trong ghi chép đã archived', async () => {
    const root = await makeWorkspace({
      'teamwork_projects/obsidian_llm_wiki/vault/Skills/cu.md': `---
title: "cu"
tags:
  - liva/skill
author: "worker"
last_update: "2026-07-28T00:00:00Z"
status: "archived"
---

Quy trình Antigravity cũ đọc GEMINI.md.
`,
    });
    const { errors, warnings } = await auditWorkspace(root);
    assert.deepEqual(errors, []);
    assert.deepEqual(warnings, []);
  });

  it('bắt dấu vết ngoại lai/lỗi thời nhưng vẫn 0 lỗi', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md': `${goodSkill('beta')}
Đọc GEMINI.md rồi mở \`.skills/\` của Antigravity, sau đó khởi động liva-gateway.
`,
    });
    const { errors, warnings } = await auditWorkspace(root);
    assert.deepEqual(errors, []);
    assert.deepEqual(codes(warnings).sort(), [
      'foreign:antigravity',
      'foreign:dot-skills-dir',
      'foreign:gemini-md',
      'foreign:legacy-node-python-stack',
    ]);
  });

  it('lẫn phương ngữ front-matter giữa SKILL.md và vault', async () => {
    const root = await makeWorkspace({
      '.claude/skills/beta/SKILL.md':
        '---\nname: beta\ntitle: "Beta"\ndescription: "x y z"\n---\n\n# beta\n',
      'teamwork_projects/obsidian_llm_wiki/vault/Rules/luat.md':
        `${goodVaultNote('luat').replace('title:', 'name: luat\ntitle:')}`,
    });
    const { errors, warnings } = await auditWorkspace(root);
    assert.deepEqual(errors, []);
    assert.deepEqual(codes(warnings), ['dialect-mixup', 'dialect-mixup']);
  });

  it('title lệch tên file chỉ là cảnh báo', async () => {
    const root = await makeWorkspace({
      'teamwork_projects/obsidian_llm_wiki/vault/Rules/luat.md': goodVaultNote('Luật Khác'),
    });
    const { errors, warnings } = await auditWorkspace(root);
    assert.deepEqual(errors, []);
    assert.deepEqual(codes(warnings), ['title-filename-mismatch']);
  });
});

describe('giao diện dòng lệnh', () => {
  let cleanRoot;
  let dirtyRoot;

  before(async () => {
    cleanRoot = await makeWorkspace({
      '.claude/skills/alpha/SKILL.md': goodSkill('alpha'),
    });
    dirtyRoot = await makeWorkspace({
      '.claude/skills/alpha/SKILL.md': goodSkill('khac-ten'),
    });
  });

  it('thoát 0 và in JSON hợp lệ khi cây sạch', async () => {
    const { stdout } = await execFileAsync(process.execPath, [
      SCRIPT,
      '--json',
      '--root',
      cleanRoot,
    ]);
    const parsed = JSON.parse(stdout);
    assert.equal(parsed.errors.length, 0);
    assert.equal(parsed.scanned.skills, 1);
  });

  it('thoát khác 0 khi có lỗi, và --json vẫn phân tích được', async () => {
    await assert.rejects(
      () => execFileAsync(process.execPath, [SCRIPT, '--json', `--root=${dirtyRoot}`]),
      (err) => {
        assert.equal(err.code, 1);
        const parsed = JSON.parse(err.stdout);
        assert.deepEqual(codes(parsed.errors), ['name-folder-mismatch']);
        return true;
      },
    );
  });
});
