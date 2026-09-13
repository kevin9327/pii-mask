/// <reference lib="webworker" />
import init, { process_bytes, reports_from_json, zip_named_files } from "./wasm/pii_wasm.js";
import type { ProcessResult, WorkerIn, WorkerOut } from "./types";

function post(msg: WorkerOut) {
  (self as DedicatedWorkerGlobalScope).postMessage(msg);
}

const ready = init().then(() => {
  post({ type: "ready" });
});

self.onmessage = async (ev: MessageEvent<WorkerIn>) => {
  await ready;
  const msg = ev.data;
  try {
    if (msg.type === "process") {
      const results: ProcessResult[] = [];
      for (let i = 0; i < msg.files.length; i++) {
        const f = msg.files[i];
        post({ type: "progress", id: msg.id, index: i, total: msg.files.length, filename: f.name });
        try {
          const bytes = new Uint8Array(f.data);
          const out = process_bytes(f.name, bytes, msg.extraRules, msg.maskMode);
          results.push({
            report: JSON.parse(out.report_json),
            masked: out.masked ?? null,
            outputName: out.output_name ?? null,
            fallbackNote: out.fallback_note ?? null,
          });
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          results.push({
            report: {
              filename: f.name,
              format: "unknown",
              warnings: [message],
              paragraph_count: 0,
              findings: [],
              summaries: [],
              diffs: [],
              confirmed: 0,
              suspicious: 0,
              already_masked: 0,
            },
            masked: null,
            outputName: null,
            fallbackNote: null,
          });
        }
      }
      post({ type: "result", id: msg.id, results });
    } else if (msg.type === "reports") {
      const obj = reports_from_json(msg.reportsJson);
      post({ type: "reports", id: msg.id, csv: obj.csv, json: obj.json, html: obj.html });
    } else if (msg.type === "zip") {
      const names: string[] = msg.files.map((f) => f.name);
      const payloads: Uint8Array[] = msg.files.map((f) => new Uint8Array(f.data));
      const data = zip_named_files(names, payloads);
      post({ type: "zip", id: msg.id, data });
    }
  } catch (err) {
    post({ type: "error", id: msg.id, message: err instanceof Error ? err.message : String(err) });
  }
};
