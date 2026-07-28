#!/usr/bin/env node
/**
 * audit-liva-skills.mjs — kiểm tra sức khoẻ cấu trúc của ba kho tri thức agent:
 *
 *   1. `.claude/skills/` + `SKILL.md`  — skill cho Claude Code
 *   2. `.agents/skills/` + `SKILL.md`  — skill cho Codex/agent khác
 *   3. `teamwork_projects/obsidian_llm_wiki/vault/{Skills,Knowledge,Rules}/*.md`
 *
 * Hai phương ngữ front-matter KHÁC NHAU và không được lẫn:
 *   - SKILL.md  : `name` + `description` (name PHẢI trùng tên thư mục chứa nó)
 *   - Vault     : `title` + `tags` + `author` + `last_update`
 *
 * LỖI (exit != 0): front-matter hỏng/thiếu khoá bắt buộc, `name` lệch tên thư mục,
 * liên kết markdown tương đối trỏ vào file không tồn tại.
 * CẢNH BÁO (exit 0): dấu vết nội dung ngoại lai/lỗi thời — di sản copy từ dự án
 * khác (GEMINI.md, Antigravity, `.skills/`) hoặc nhắc tới stack đã bị xoá khỏi
 * repo (liva-gateway/liva-ai-engine Node.js + Python).
 *
 * Chỉ dùng thư viện chuẩn của Node và I/O bất đồng bộ — không `fs*Sync`,
 * không thêm dependency (cùng lý do với `scripts/docs-check.mjs`).
 *
 * Dùng:
 *   node scripts/audit-liva-skills.mjs            # báo cáo cho người đọc
 *   node scripts/audit-liva-skills.mjs --json     # máy đọc
 *   node scripts/audit-liva-skills.mjs --root DIR # kiểm một cây khác (test dùng)
 */

import { readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const SKILL_ROOTS = ['.claude/skills', '.agents/skills'];
export const VAULT_ROOT = 'teamwork_projects/obsidian_llm_wiki/vault';
export const VAULT_SUBDIRS = ['Skills', 'Knowledge', 'Rules'];

/** Khoá bắt buộc theo từng phương ngữ front-matter. */
export const SKILL_REQUIRED_KEYS = ['name', 'description'];
export const VAULT_REQUIRED_KEYS = ['title', 'tags', 'author', 'last_update'];

/**
 * Dấu vết nội dung ngoại lai / lỗi thời.
 * Mỗi mục nêu LÝ DO để người đọc biết vì sao nó đáng cảnh báo — cảnh báo không
 * kèm lý do sẽ bị tắt đi trong vòng một tuần.
 */
export const FOREIGN_MARKERS = [
  {
    id: 'gemini-md',
    pattern: /GEMINI\.md/,
    why: 'Repo LIVA dùng CLAUDE.md + AGENTS.md; GEMINI.md là di sản của một dự án khác.',
  },
  {
    id: 'antigravity',
    pattern: /Antigravity/i,
    why: 'Antigravity-Vibe là dự án khác — quy trình của nó không áp dụng cho LIVA.',
  },
  {
    id: 'dot-skills-dir',
    pattern: /(^|[\s`([])\.skills\//,
    why: 'LIVA đặt skill ở `.claude/skills/` và `.agents/skills/`, không có thư mục `.skills/`.',
  },
  {
    id: 'gemini-home',
    pattern: /~\/\.gemini/,
    why: 'Đường dẫn cấu hình của công cụ khác, không tồn tại trong quy trình LIVA.',
  },
  {
    id: 'legacy-node-python-stack',
    pattern: /liva-gateway|liva-ai-engine/,
    why: 'Stack Node.js/Python cũ đã bị xoá; AGENTS.md cấm khôi phục hoặc chạy lại.',
  },
];

/** Bỏ qua các thư mục này khi quét. */
const IGNORED_DIRS = new Set(['node_modules', '.git', 'dist', 'target', 'Templates']);

class Finding {
  constructor(level, code, file, message, extra = {}) {
    this.level = level; // 'error' | 'warning'
    this.code = code;
    this.file = file;
    this.message = message;
    Object.assign(this, extra);
  }
}

const error = (code, file, message, extra) => new Finding('error', code, file, message, extra);
const warning = (code, file, message, extra) => new Finding('warning', code, file, message, extra);

/** `path.relative` + dấu gạch chéo xuôi, để báo cáo giống nhau trên mọi OS. */
export function toPosix(relPath) {
  return relPath.split(path.sep).join('/');
}

async function exists(target) {
  try {
    await stat(target);
    return true;
  } catch {
    return false;
  }
}

/**
 * Bóc front-matter YAML tối giản (không dùng js-yaml để khỏi thêm dependency).
 * Đủ cho tập khoá thực tế trong repo: scalar, danh sách gạch đầu dòng, mảng
 * inline `tags: ["a", "b"]`, và danh sách các bản ghi thụt lề (`inputs:` trong
 * `vault/Skills/web_search.md`).
 *
 * Chỉ khoá ở cột 0 mới là khoá cấp cao nhất — đó là những khoá mà bộ kiểm bắt buộc.
 *
 * @returns {{ok: true, data: Record<string, unknown>, endLine: number}
 *          | {ok: false, reason: string, line: number}}
 */
export function parseFrontmatter(content) {
  const lines = content.split(/\r?\n/);
  if (lines[0]?.trim() !== '---') {
    return { ok: false, reason: 'thiếu khối front-matter mở đầu bằng `---`', line: 1 };
  }

  let closingLine = -1;
  for (let i = 1; i < lines.length; i += 1) {
    if (lines[i].trim() === '---') {
      closingLine = i;
      break;
    }
  }
  if (closingLine === -1) {
    return { ok: false, reason: 'front-matter không có dòng `---` đóng lại', line: lines.length };
  }

  const data = {};
  let currentKey = null;
  let currentRecord = null;

  for (let i = 1; i < closingLine; i += 1) {
    const raw = lines[i];
    const lineNo = i + 1;
    if (raw.trim() === '' || raw.trim().startsWith('#')) continue;

    const indent = raw.length - raw.trimStart().length;

    if (indent === 0) {
      const keyValue = raw.match(/^([A-Za-z_][\w-]*)\s*:\s*(.*)$/);
      if (!keyValue) {
        return {
          ok: false,
          reason: `dòng không phải \`khoá: giá trị\`: ${raw.trim()}`,
          line: lineNo,
        };
      }
      const [, key, rest] = keyValue;
      if (Object.prototype.hasOwnProperty.call(data, key)) {
        return { ok: false, reason: `khoá lặp lại: ${key}`, line: lineNo };
      }

      currentKey = key;
      currentRecord = null;
      const value = rest.trim();

      if (value === '') {
        data[key] = [];
      } else if (value.startsWith('[') && value.endsWith(']')) {
        const inner = value.slice(1, -1).trim();
        data[key] = inner === '' ? [] : inner.split(',').map((item) => stripScalar(item.trim()));
        currentKey = null;
      } else {
        data[key] = stripScalar(value);
        currentKey = null;
      }
      continue;
    }

    if (!currentKey) {
      return { ok: false, reason: 'dòng thụt lề không thuộc khoá nào', line: lineNo };
    }

    const listItem = raw.match(/^\s*-\s+(.*)$/);
    if (listItem) {
      const item = listItem[1].trim();
      const nested = item.match(/^([A-Za-z_][\w-]*)\s*:\s*(.*)$/);
      if (nested) {
        currentRecord = { [nested[1]]: stripScalar(nested[2]) };
        data[currentKey].push(currentRecord);
      } else {
        currentRecord = null;
        data[currentKey].push(stripScalar(item));
      }
      continue;
    }

    const nestedPair = raw.match(/^\s+([A-Za-z_][\w-]*)\s*:\s*(.*)$/);
    if (nestedPair) {
      if (currentRecord) currentRecord[nestedPair[1]] = stripScalar(nestedPair[2]);
      continue;
    }

    return { ok: false, reason: `dòng thụt lề không hợp lệ: ${raw.trim()}`, line: lineNo };
  }

  return { ok: true, data, endLine: closingLine + 1 };
}

function stripScalar(value) {
  const trimmed = value.trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

/** Liệt kê mọi file khớp `predicate` bên dưới `dir`. */
export async function walk(dir, predicate, acc = []) {
  let entries;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch {
    return acc;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (IGNORED_DIRS.has(entry.name)) continue;
      await walk(full, predicate, acc);
    } else if (entry.isFile() && predicate(entry.name)) {
      acc.push(full);
    }
  }
  return acc;
}

/**
 * Liên kết markdown tương đối trỏ vào đâu.
 * Bỏ qua http(s)/mailto/anchor thuần và các sơ đồ giao thức khác.
 */
export function extractRelativeLinks(content) {
  const links = [];
  const linkPattern = /\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
  let match;
  while ((match = linkPattern.exec(content)) !== null) {
    const target = match[1];
    if (/^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
    if (target.startsWith('#')) continue;
    if (target.startsWith('//')) continue;
    const withoutAnchor = target.split('#')[0];
    if (withoutAnchor === '') continue;
    let decoded;
    try {
      decoded = decodeURIComponent(withoutAnchor);
    } catch {
      decoded = withoutAnchor;
    }
    links.push({ raw: target, target: decoded });
  }
  return links;
}

async function checkLinks(absFile, content, root, findings) {
  const relFile = toPosix(path.relative(root, absFile));
  for (const link of extractRelativeLinks(content)) {
    const resolved = path.resolve(path.dirname(absFile), link.target);
    if (!(await exists(resolved))) {
      findings.push(
        error('broken-link', relFile, `liên kết tương đối hỏng: ${link.raw}`, { link: link.raw }),
      );
    }
  }
}

function checkForeignMarkers(relFile, content, findings) {
  const lines = content.split(/\r?\n/);
  for (const marker of FOREIGN_MARKERS) {
    const hitLine = lines.findIndex((line) => marker.pattern.test(line));
    if (hitLine !== -1) {
      findings.push(
        warning(
          `foreign:${marker.id}`,
          relFile,
          `nội dung ngoại lai/lỗi thời ở dòng ${hitLine + 1} — ${marker.why}`,
          { line: hitLine + 1 },
        ),
      );
    }
  }
}

/** Kiểm một `SKILL.md` (phương ngữ Codex/Claude). */
export async function auditSkillFile(absFile, root, findings) {
  const relFile = toPosix(path.relative(root, absFile));
  const content = await readFile(absFile, 'utf8');
  const parsed = parseFrontmatter(content);

  if (
    /^\s*description:\s*\[TODO:/im.test(content) ||
    /^## Structuring This Skill\s*$/im.test(content) ||
    /^\s*\[TODO:[^\]]+\]\s*$/im.test(content)
  ) {
    findings.push(
      error(
        'template-placeholder',
        relFile,
        'SKILL.md còn placeholder của scaffold; phải hoàn thiện nội dung trước khi báo xanh',
      ),
    );
  }

  if (!parsed.ok) {
    findings.push(
      error('bad-frontmatter', relFile, `front-matter hỏng (dòng ${parsed.line}): ${parsed.reason}`),
    );
  } else {
    for (const key of SKILL_REQUIRED_KEYS) {
      const value = parsed.data[key];
      if (value === undefined || (typeof value === 'string' && value.trim() === '')) {
        findings.push(error('missing-key', relFile, `front-matter thiếu khoá bắt buộc \`${key}\``));
      }
    }
    if (Object.prototype.hasOwnProperty.call(parsed.data, 'title')) {
      findings.push(
        warning(
          'dialect-mixup',
          relFile,
          'SKILL.md dùng khoá `title` của phương ngữ vault — SKILL.md chỉ nhận `name` + `description`',
        ),
      );
    }
    const folder = path.basename(path.dirname(absFile));
    const name = parsed.data.name;
    if (typeof name === 'string' && name.trim() !== '' && name.trim() !== folder) {
      findings.push(
        error(
          'name-folder-mismatch',
          relFile,
          `\`name: ${name}\` lệch tên thư mục \`${folder}\` — harness nạp skill theo tên thư mục`,
        ),
      );
    }
  }

  await checkLinks(absFile, content, root, findings);
  checkForeignMarkers(relFile, content, findings);
}

/** Kiểm một ghi chép vault (phương ngữ Obsidian). */
export async function auditVaultFile(absFile, root, findings) {
  const relFile = toPosix(path.relative(root, absFile));
  const content = await readFile(absFile, 'utf8');
  const parsed = parseFrontmatter(content);
  let archived = false;

  if (!parsed.ok) {
    findings.push(
      error('bad-frontmatter', relFile, `front-matter hỏng (dòng ${parsed.line}): ${parsed.reason}`),
    );
  } else {
    archived = String(parsed.data.status ?? '').toLowerCase() === 'archived';
    for (const key of VAULT_REQUIRED_KEYS) {
      const value = parsed.data[key];
      const empty =
        value === undefined ||
        (typeof value === 'string' && value.trim() === '') ||
        (Array.isArray(value) && value.length === 0);
      if (empty) {
        findings.push(error('missing-key', relFile, `front-matter thiếu khoá bắt buộc \`${key}\``));
      }
    }
    if (Object.prototype.hasOwnProperty.call(parsed.data, 'name')) {
      findings.push(
        warning(
          'dialect-mixup',
          relFile,
          'ghi chép vault dùng khoá `name` của phương ngữ SKILL.md — vault nhận `title` + `tags`',
        ),
      );
    }
    const title = parsed.data.title;
    const base = path.basename(absFile, '.md');
    if (typeof title === 'string' && title.trim() !== '' && title.trim() !== base) {
      findings.push(
        warning(
          'title-filename-mismatch',
          relFile,
          `\`title: ${title}\` lệch tên file \`${base}\` — search_vault cộng điểm theo title`,
        ),
      );
    }
  }

  await checkLinks(absFile, content, root, findings);
  if (!archived) checkForeignMarkers(relFile, content, findings);
}

/**
 * Chạy toàn bộ đợt kiểm trên một cây thư mục.
 * @param {string} root Gốc workspace (mặc định: thư mục hiện hành).
 */
export async function auditWorkspace(root = process.cwd()) {
  const absRoot = path.resolve(root);
  const findings = [];
  const scanned = { skills: 0, vault: 0 };

  for (const skillRoot of SKILL_ROOTS) {
    const dir = path.join(absRoot, skillRoot);
    if (!(await exists(dir))) {
      findings.push(
        error(
          'missing-root',
          toPosix(skillRoot),
          'thiếu thư mục skill bắt buộc; không được coi cây rỗng là kết quả hợp lệ',
        ),
      );
      continue;
    }
    const files = await walk(dir, (name) => name === 'SKILL.md');
    files.sort();
    for (const file of files) {
      scanned.skills += 1;
      await auditSkillFile(file, absRoot, findings);
    }
  }

  for (const sub of VAULT_SUBDIRS) {
    const dir = path.join(absRoot, VAULT_ROOT, sub);
    if (!(await exists(dir))) {
      const relDir = toPosix(path.join(VAULT_ROOT, sub));
      findings.push(
        error(
          'missing-root',
          relDir,
          'thiếu thư mục vault bắt buộc; không được coi cây rỗng là kết quả hợp lệ',
        ),
      );
      continue;
    }
    const files = await walk(dir, (name) => name.endsWith('.md'));
    files.sort();
    for (const file of files) {
      scanned.vault += 1;
      await auditVaultFile(file, absRoot, findings);
    }
  }

  const errors = findings.filter((f) => f.level === 'error');
  const warnings = findings.filter((f) => f.level === 'warning');
  return { root: toPosix(absRoot), scanned, findings, errors, warnings };
}

export function formatReport(result) {
  const lines = [];
  lines.push(`Đã quét ${result.scanned.skills} SKILL.md và ${result.scanned.vault} ghi chép vault.`);
  if (result.errors.length === 0 && result.warnings.length === 0) {
    lines.push('Không có lỗi, không có cảnh báo.');
    return lines.join('\n');
  }
  for (const group of [
    { label: 'LỖI', items: result.errors },
    { label: 'CẢNH BÁO', items: result.warnings },
  ]) {
    if (group.items.length === 0) continue;
    lines.push('');
    lines.push(`${group.label} (${group.items.length}):`);
    for (const item of group.items) {
      lines.push(`  [${item.code}] ${item.file} — ${item.message}`);
    }
  }
  lines.push('');
  lines.push(`Tổng: ${result.errors.length} lỗi · ${result.warnings.length} cảnh báo.`);
  return lines.join('\n');
}

export function parseArgs(argv) {
  const options = { json: false, root: process.cwd() };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') options.json = true;
    else if (arg === '--root') {
      options.root = argv[i + 1];
      i += 1;
    } else if (arg.startsWith('--root=')) options.root = arg.slice('--root='.length);
  }
  return options;
}

async function main(argv) {
  const options = parseArgs(argv);
  const result = await auditWorkspace(options.root);
  if (options.json) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } else {
    process.stdout.write(`${formatReport(result)}\n`);
  }
  return result.errors.length === 0 ? 0 : 1;
}

const invokedDirectly = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (invokedDirectly) {
  main(process.argv.slice(2))
    .then((code) => {
      process.exitCode = code;
    })
    .catch((err) => {
      process.stderr.write(`audit-liva-skills: ${err.stack || err.message}\n`);
      process.exitCode = 2;
    });
}
