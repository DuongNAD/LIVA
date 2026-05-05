import { ModuleKind, Project, ScriptTarget } from "ts-morph";
import { CSHS_WEIGHTS } from "./harness-types";

export interface CSHSResult {
  score: number;
  anchors: string[];
  diagnosticCount: number;
  pass: boolean;
}

const PLACEHOLDER_TOKENS = ["YOUR_CODE_HERE", "TODO:", "FIXME:", "PLACEHOLDER"] as const;

export class CSHSAnalyzer {
  #project: Project;

  constructor() {
    this.#project = new Project({
      useInMemoryFileSystem: true,
      compilerOptions: {
        target: ScriptTarget.ESNext,
        module: ModuleKind.ESNext,
        strict: true,
        noEmit: true,
        skipLibCheck: true,
        noUnusedLocals: true,
        allowJs: false,
      },
    });
  }

  public analyze(astDiff: string, jobId: string, threshold: number): CSHSResult {
    const fileName = `/eval_${jobId}.ts`;
    const sourceFile = this.#project.createSourceFile(fileName, astDiff, { overwrite: true });

    try {
      let totalPenalty = 0;
      const anchors: string[] = [];

      for (const token of PLACEHOLDER_TOKENS) {
        const count = this.#countOccurrences(astDiff, token);
        if (count === 0) continue;
        totalPenalty += CSHS_WEIGHTS.PLACEHOLDER_TOKEN * count;
        anchors.push(`Placeholder "${token}" detected`);
      }

      const diagnostics = sourceFile.getPreEmitDiagnostics().filter((diag) => {
        const diagSource = diag.getSourceFile();
        return !diagSource || diagSource.getFilePath() === sourceFile.getFilePath();
      });

      for (const diag of diagnostics) {
        const code = diag.getCode();
        switch (code) {
          case 2304:
            totalPenalty += CSHS_WEIGHTS.TS2304_UNDEFINED_NAME;
            anchors.push(`TS2304: ${diag.getMessageText()}`);
            break;
          case 2307:
            totalPenalty += CSHS_WEIGHTS.TS2307_UNRESOLVED_IMPORT;
            anchors.push(`TS2307: ${diag.getMessageText()}`);
            break;
          case 7027:
            totalPenalty += CSHS_WEIGHTS.TS7027_UNREACHABLE_CODE;
            anchors.push("TS7027: Unreachable code");
            break;
          case 6133:
            totalPenalty += CSHS_WEIGHTS.TS6133_UNUSED_VARIABLE;
            anchors.push(`TS6133: ${diag.getMessageText()}`);
            break;
          default:
            totalPenalty += code < 2000 ? 0.3 : 0.05;
            break;
        }
      }

      const score = Number(Math.min(totalPenalty, 1).toFixed(3));

      return {
        score,
        anchors: anchors.slice(0, 20),
        diagnosticCount: diagnostics.length,
        pass: score <= threshold,
      };
    } finally {
      this.#project.removeSourceFile(sourceFile);
    }
  }

  #countOccurrences(source: string, token: string): number {
    let count = 0;
    let index = source.indexOf(token);
    while (index !== -1) {
      count += 1;
      index = source.indexOf(token, index + token.length);
    }
    return count;
  }
}
