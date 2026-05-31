import { ASTWorkerBridge } from "../core/ASTWorkerBridge";

export interface CSHSResult {
  score: number;
  anchors: string[];
  diagnosticCount: number;
  pass: boolean;
}

export class CSHSAnalyzer {
  public async analyze(astDiff: string, jobId: string, threshold: number): Promise<CSHSResult> {
      return ASTWorkerBridge.analyzeCSHS(astDiff, jobId, threshold);
  }
}
