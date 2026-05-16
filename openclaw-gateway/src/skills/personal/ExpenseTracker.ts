import { safeRename } from '../../utils/FileUtils';
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { logger } from "@utils/logger";
import { SkillMetadata } from "../SkillMetadata";

const EXPENSE_FILE = path.join(os.homedir(), ".liva", "expenses.json");

interface Expense {
  id: string;
  amount: number;
  description: string;
  category: string;
  date: string; // ISO string
}

export const metadata: SkillMetadata = {
  name: "expense_tracker",
  category: "personal",
  short_desc: "Track user expenses.",
  semantic_tags: ["#expense", "#money", "#chitieu", "#finance", "#tien"],
  search_keywords: ["expense", "chi tiêu", "tiền", "mua", "thanh toán", "money", "budget", "ngân sách"],
  description: "[AUTO_RUN] Personal expense tracker. Add expenses, view spending history, and get summaries by period. Data stored locally.",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["add", "list", "summary"],
        description: "'add' = record a new expense, 'list' = show recent expenses, 'summary' = spending summary by category.",
      },
      amount: { type: "number", description: "Amount spent (required for 'add'). Unit: VND." },
      description: { type: "string", description: "What was purchased (required for 'add')." },
      category: {
        type: "string",
        enum: ["food", "transport", "shopping", "bills", "entertainment", "education", "health", "other"],
        description: "Expense category (default: 'other').",
      },
      period: {
        type: "string",
        enum: ["today", "week", "month", "all"],
        description: "Time period for 'list' and 'summary' (default: 'month').",
      },
    },
    required: ["action"],
  },
};

async function loadExpenses(): Promise<Expense[]> {
  try {
    const raw = await fsp.readFile(EXPENSE_FILE, "utf-8");
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

async function saveExpenses(expenses: Expense[]): Promise<void> {
  const dir = path.dirname(EXPENSE_FILE);
  await fsp.mkdir(dir, { recursive: true });
  
  // Atomic write to prevent file corruption
  const tmpFile = `${EXPENSE_FILE}.tmp`;
  await fsp.writeFile(tmpFile, JSON.stringify(expenses, null, 2), "utf-8");
  await safeRename(tmpFile, EXPENSE_FILE);
}

function filterByPeriod(expenses: Expense[], period: string): Expense[] {
  const now = new Date();
  const start = new Date();

  switch (period) {
    case "today":
      start.setHours(0, 0, 0, 0);
      break;
    case "week":
      start.setDate(now.getDate() - 7);
      break;
    case "month":
      start.setMonth(now.getMonth() - 1);
      break;
    case "all":
      return expenses;
    default:
      start.setMonth(now.getMonth() - 1);
  }

  return expenses.filter(e => new Date(e.date) >= start);
}

function formatVND(amount: number): string {
  return amount.toLocaleString("vi-VN") + "đ";
}

export const execute = async (args: {
  action: "add" | "list" | "summary";
  amount?: number;
  description?: string;
  category?: string;
  period?: string;
}): Promise<string> => {
  const expenses = await loadExpenses();

  if (args.action === "add") {
    if (!args.amount || args.amount <= 0) return "Error: Please provide a valid amount > 0.";
    if (!args.description?.trim()) return "Error: Please describe what you spent on.";

    const newExpense: Expense = {
      id: `exp_${Date.now()}`,
      amount: args.amount,
      description: args.description.trim(),
      category: args.category || "other",
      date: new Date().toISOString(),
    };

    expenses.push(newExpense);
    await saveExpenses(expenses);

    logger.info(`[Skill: expense_tracker] Added: ${formatVND(args.amount)} - ${newExpense.description}`);

    // Calculate today's total
    const todayExpenses = filterByPeriod(expenses, "today");
    const todayTotal = todayExpenses.reduce((sum, e) => sum + e.amount, 0);

    return `✅ Expense recorded: ${formatVND(args.amount)} — "${newExpense.description}" [${newExpense.category}]\n📊 Today's total: ${formatVND(todayTotal)} (${todayExpenses.length} expenses)`;
  }

  if (args.action === "list") {
    const period = args.period || "month";
    const filtered = filterByPeriod(expenses, period);

    if (filtered.length === 0) return `No expenses found for period: ${period}.`;

    const sorted = filtered.sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());
    const display = sorted.slice(0, 20); // Max 20 entries

    let report = `[EXPENSE LIST — ${period}] (${filtered.length} entries)\n\n`;
    display.forEach((e, i) => {
      const date = new Date(e.date).toLocaleDateString("vi-VN");
      report += `${i + 1}. ${date} | ${formatVND(e.amount)} | ${e.description} [${e.category}]\n`;
    });

    const total = filtered.reduce((sum, e) => sum + e.amount, 0);
    report += `\n💰 Total: ${formatVND(total)}`;

    return report;
  }

  if (args.action === "summary") {
    const period = args.period || "month";
    const filtered = filterByPeriod(expenses, period);

    if (filtered.length === 0) return `No expenses found for period: ${period}.`;

    // Group by category
    const byCategory: Record<string, number> = {};
    for (const e of filtered) {
      byCategory[e.category] = (byCategory[e.category] || 0) + e.amount;
    }

    const total = filtered.reduce((sum, e) => sum + e.amount, 0);
    const sorted = Object.entries(byCategory).sort(([, a], [, b]) => b - a);

    let report = `[EXPENSE SUMMARY — ${period}]\n\n`;
    sorted.forEach(([cat, amount]) => {
      const percent = ((amount / total) * 100).toFixed(1);
      report += `📌 ${cat}: ${formatVND(amount)} (${percent}%)\n`;
    });
    report += `\n💰 Grand Total: ${formatVND(total)} across ${filtered.length} expenses`;

    return report;
  }

  return "Error: Invalid action. Use 'add', 'list', or 'summary'.";
};
