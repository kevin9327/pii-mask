export type Confidence = "confirmed" | "suspicious" | "already_masked";

export interface Finding {
  rule_id: string;
  label: string;
  raw: string;
  masked_preview: string;
  confidence: Confidence;
  paragraph_index: number;
  context_before: string;
  context_after: string;
}

export interface TypeSummary {
  rule_id: string;
  label: string;
  count: number;
  confirmed: number;
  suspicious: number;
  already_masked: number;
  preview: string;
}

export interface DiffHunk {
  paragraph_index: number;
  before: string;
  after: string;
}

export interface FileReport {
  filename: string;
  format: string;
  warnings: string[];
  paragraph_count: number;
  findings: Finding[];
  summaries: TypeSummary[];
  diffs: DiffHunk[];
  confirmed: number;
  suspicious: number;
  already_masked: number;
}

export interface ProcessResult {
  report: FileReport;
  masked: Uint8Array | null;
  outputName: string | null;
  fallbackNote: string | null;
}

export type FileBlob = { name: string; data: ArrayBuffer };

export type WorkerRequest =
  | { type: "process"; files: FileBlob[]; extraRules: string; maskMode: string }
  | { type: "reports"; reportsJson: string }
  | { type: "zip"; files: FileBlob[] };

export type WorkerIn = WorkerRequest & { id: number };

export type WorkerOut =
  | { type: "ready" }
  | { type: "progress"; id: number; index: number; total: number; filename: string }
  | { type: "result"; id: number; results: ProcessResult[] }
  | { type: "reports"; id: number; csv: string; json: string; html: string }
  | { type: "zip"; id: number; data: Uint8Array }
  | { type: "error"; id: number; message: string };
