import { SkillMetadata } from "../SkillMetadata";
import { logger } from "../../utils/logger";
import { z } from "zod";

export const metadata: SkillMetadata = {
  name: "evaluate_math_expression",
  category: "core",
  short_desc: "Evaluate a mathematical expression securely.",
  description: "Evaluate a mathematical expression securely using a recursive descent parser. Supports operators +, -, *, /, %, ^, parentheses, constants pi, e, and functions: sin, cos, tan, log, ln, sqrt, abs, exp.",
  parameters: {
    type: "object",
    properties: {
      expression: {
        type: "string",
        description: "The mathematical expression to evaluate (e.g. '2 * (3 + pi) - sqrt(9)')."
      }
    },
    required: ["expression"]
  }
};

interface Token {
  type: 'NUMBER' | 'OP' | 'ID' | 'EOF';
  value: string;
}

class Tokenizer {
  private input: string;
  private pos = 0;

  constructor(input: string) {
    this.input = input;
  }

  public nextToken(): Token {
    this.skipWhitespace();
    if (this.pos >= this.input.length) {
      return { type: 'EOF', value: '' };
    }

    const char = this.input[this.pos];

    // Numbers (including decimals)
    if (/[0-9.]/.test(char)) {
      let numStr = '';
      let hasDot = false;
      while (this.pos < this.input.length && /[0-9.]/.test(this.input[this.pos])) {
        const nextChar = this.input[this.pos];
        if (nextChar === '.') {
          if (hasDot) {
            throw new Error(`Invalid number format (multiple decimal points)`);
          }
          hasDot = true;
        }
        numStr += nextChar;
        this.pos++;
      }
      return { type: 'NUMBER', value: numStr };
    }

    // Identifiers (functions or constants)
    if (/[a-zA-Z]/.test(char)) {
      let idStr = '';
      while (this.pos < this.input.length && /[a-zA-Z]/.test(this.input[this.pos])) {
        idStr += this.input[this.pos];
        this.pos++;
      }
      return { type: 'ID', value: idStr.toLowerCase() };
    }

    // Operators
    if (['+', '-', '*', '/', '%', '^', '(', ')'].includes(char)) {
      this.pos++;
      return { type: 'OP', value: char };
    }

    throw new Error(`Unexpected character: '${char}'`);
  }

  private skipWhitespace() {
    while (this.pos < this.input.length && /\s/.test(this.input[this.pos])) {
      this.pos++;
    }
  }
}

class Parser {
  private tokenizer: Tokenizer;
  private currentToken: Token;
  private depth = 0;

  constructor(expression: string) {
    this.tokenizer = new Tokenizer(expression);
    this.currentToken = this.tokenizer.nextToken();
  }

  private eat(type: 'NUMBER' | 'OP' | 'ID' | 'EOF', value?: string) {
    if (this.currentToken.type === type && (value === undefined || this.currentToken.value === value)) {
      this.currentToken = this.tokenizer.nextToken();
    } else {
      throw new Error(`Unexpected token: '${this.currentToken.value || this.currentToken.type}'`);
    }
  }

  public parse(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      const val = this.expression();
      if (this.currentToken.type !== 'EOF') {
        throw new Error(`Unexpected trailing characters: '${this.currentToken.value}'`);
      }
      return val;
    } finally {
      this.depth--;
    }
  }

  // expression -> term (('+' | '-') term)*
  private expression(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      let result = this.term();
      while (this.currentToken.type === 'OP' && (this.currentToken.value === '+' || this.currentToken.value === '-')) {
        const op = this.currentToken.value;
        this.eat('OP');
        const right = this.term();
        if (op === '+') result += right;
        else result -= right;
      }
      return result;
    } finally {
      this.depth--;
    }
  }

  // term -> power (('*' | '/' | '%') power)*
  private term(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      let result = this.power();
      while (this.currentToken.type === 'OP' && (this.currentToken.value === '*' || this.currentToken.value === '/' || this.currentToken.value === '%')) {
        const op = this.currentToken.value;
        this.eat('OP');
        const right = this.power();
        if (op === '*') {
          result *= right;
        } else if (op === '/') {
          if (right === 0) throw new Error("Division by zero");
          result /= right;
        } else {
          if (right === 0) throw new Error("Modulo by zero");
          result %= right;
        }
      }
      return result;
    } finally {
      this.depth--;
    }
  }

  // power -> unary ('^' power)?
  private power(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      const left = this.unary();
      if (this.currentToken.type === 'OP' && this.currentToken.value === '^') {
        this.eat('OP');
        const right = this.power();
        return Math.pow(left, right);
      }
      return left;
    } finally {
      this.depth--;
    }
  }

  // unary -> ('+' | '-') unary | factor
  private unary(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      if (this.currentToken.type === 'OP' && (this.currentToken.value === '+' || this.currentToken.value === '-')) {
        const op = this.currentToken.value;
        this.eat('OP');
        const val = this.unary();
        return op === '+' ? val : -val;
      }
      return this.factor();
    } finally {
      this.depth--;
    }
  }

  // factor -> NUMBER | ID | '(' expression ')'
  private factor(): number {
    this.depth++;
    if (this.depth > 300) {
      throw new Error("Maximum recursion depth exceeded");
    }
    try {
      const token = this.currentToken;
      if (token.type === 'NUMBER') {
        const val = parseFloat(token.value);
        if (isNaN(val)) throw new Error(`Invalid number: ${token.value}`);
        this.eat('NUMBER');
        return val;
      }

      if (token.type === 'ID') {
        const id = token.value;
        this.eat('ID');
        
        const allowedFunctions = ['sin', 'cos', 'tan', 'log', 'ln', 'sqrt', 'abs', 'exp'];
        const allowedConstants = ['pi', 'e'];

        if (allowedConstants.includes(id)) {
          if (id === 'pi') return Math.PI;
          if (id === 'e') return Math.E;
        }

        if (allowedFunctions.includes(id)) {
          this.eat('OP', '(');
          const arg = this.expression();
          this.eat('OP', ')');

          switch (id) {
            case 'sin': return Math.sin(arg);
            case 'cos': return Math.cos(arg);
            case 'tan': return Math.tan(arg);
            case 'log': return Math.log10(arg);
            case 'ln': return Math.log(arg);
            case 'sqrt':
              if (arg < 0) throw new Error("Square root of a negative number");
              return Math.sqrt(arg);
            case 'abs': return Math.abs(arg);
            case 'exp': return Math.exp(arg);
          }
        }

        throw new Error(`Unknown identifier: ${id}`);
      }

      if (token.type === 'OP' && token.value === '(') {
        this.eat('OP', '(');
        const val = this.expression();
        this.eat('OP', ')');
        return val;
      }

      throw new Error(`Unexpected token in factor: ${token.value || token.type}`);
    } finally {
      this.depth--;
    }
  }
}

const argsSchema = z.object({
  expression: z.string({
    message: "No mathematical expression provided.",
  })
  .trim()
  .min(1, "No mathematical expression provided.")
  .max(500, "Expression exceeds maximum length of 500 characters.")
});

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = argsSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `Error: ${parsed.error.issues.map(e => e.message).join(", ")}`;
  }
  const args = parsed.data;

  const expr = args.expression;

  // Basic character whitelist validation to prevent any potential exploit
  const allowedChars = /^[0-9a-zA-Z\s.+\-*/%^()]+$/;
  if (!allowedChars.test(expr)) {
    return "Error: Expression contains forbidden characters.";
  }

  try {
    logger.info(`[Skill: evaluate_math_expression] Evaluating expression: ${expr}`);
    const parser = new Parser(expr);
    const result = parser.parse();
    if (!Number.isFinite(result)) {
      return "Error: The calculation resulted in an undefined or infinite value.";
    }
    return String(result);
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[Skill: evaluate_math_expression] Error: ${errMsg}`);
    return `Error: ${errMsg}`;
  }
};
