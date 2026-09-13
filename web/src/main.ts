import type { FileReport, ProcessResult, WorkerOut, WorkerRequest } from "./types";

const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });

const drop = document.getElementById("drop") as HTMLElement;
const fileInput = document.getElementById("file-input") as HTMLInputElement;
const pick = document.getElementById("pick") as HTMLButtonElement;
const folderInput = document.getElementById("folder-input") as HTMLInputElement;
const pickFolder = document.getElementById("pick-folder") as HTMLButtonElement;
const runBtn = document.getElementById("run") as HTMLButtonElement;
const clearBtn = document.getElementById("clear") as HTMLButtonElement;
const statusEl = document.getElementById("status") as HTMLElement;
const rows = document.getElementById("rows") as HTMLElement;
const diffs = document.getElementById("diffs") as HTMLElement;
const extraRules = document.getElementById("extra-rules") as HTMLTextAreaElement;
const maskMode = document.getElementById("mask-mode") as HTMLSelectElement;
const fileDls = document.getElementById("file-dls") as HTMLElement;
const dlMasked = document.getElementById("dl-masked") as HTMLButtonElement;
const dlCsv = document.getElementById("dl-csv") as HTMLButtonElement;
const dlJson = document.getElementById("dl-json") as HTMLButtonElement;
const dlHtml = document.getElementById("dl-html") as HTMLButtonElement;

let workerReady = false;
let files: { name: string; data: ArrayBuffer }[] = [];
let last: ProcessResult[] = [];
let reqId = 1;
const pending = new Map<number, (msg: WorkerOut) => void>();

worker.onmessage = (ev: MessageEvent<WorkerOut>) => {
  const msg = ev.data;
  if (msg.type === "ready") {
    workerReady = true;
    statusEl.textContent = "준비됨. 파일을 놓으면 이 기기에서만 처리합니다.";
    return;
  }
  if (msg.type === "progress") {
    statusEl.textContent = `처리 중 ${msg.index + 1}/${msg.total} — ${msg.filename}`;
    return;
  }
  const wait = pending.get(msg.id);
  if (wait) {
    pending.delete(msg.id);
    wait(msg);
  }
};

function call(payload: WorkerRequest): Promise<WorkerOut> {
  const id = reqId++;
  return new Promise((resolve, reject) => {
    pending.set(id, (msg) => {
      if (msg.type === "error") reject(new Error(msg.message));
      else resolve(msg);
    });
    worker.postMessage({ ...payload, id });
  });
}

function blobPart(data: Uint8Array): BlobPart {
  return data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) as ArrayBuffer;
}

pick.onclick = () => fileInput.click();
pickFolder.onclick = () => folderInput.click();
fileInput.onchange = async () => {
  if (fileInput.files) await addFileList(fileInput.files);
  fileInput.value = "";
};
folderInput.onchange = async () => {
  if (folderInput.files) await addFileList(folderInput.files);
  folderInput.value = "";
};

drop.addEventListener("dragover", (e) => {
  e.preventDefault();
  drop.classList.add("drag");
});
drop.addEventListener("dragleave", () => drop.classList.remove("drag"));
drop.addEventListener("drop", async (e) => {
  e.preventDefault();
  drop.classList.remove("drag");
  const dt = e.dataTransfer;
  if (!dt) return;
  await addFromDataTransfer(dt);
});

async function addFromDataTransfer(dt: DataTransfer) {
  const collected: { name: string; data: ArrayBuffer }[] = [];
  const items = dt.items;
  if (items && items.length) {
    const entries: FileSystemEntry[] = [];
    for (let i = 0; i < items.length; i++) {
      const entry = items[i].webkitGetAsEntry?.();
      if (entry) entries.push(entry);
    }
    if (entries.length) {
      for (const entry of entries) await walkEntry(entry, collected);
      files.push(...collected);
      afterFiles();
      return;
    }
  }
  if (dt.files) await addFileList(dt.files);
}

async function walkEntry(entry: FileSystemEntry, out: { name: string; data: ArrayBuffer }[]) {
  if (entry.isFile) {
    const file = await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej));
    out.push({ name: file.name, data: await file.arrayBuffer() });
  } else if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    const children: FileSystemEntry[] = [];
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
      if (!batch.length) break;
      children.push(...batch);
    }
    for (const child of children) await walkEntry(child, out);
  }
}

async function addFileList(list: FileList) {
  for (const file of Array.from(list)) {
    files.push({ name: file.name, data: await file.arrayBuffer() });
  }
  afterFiles();
}

function afterFiles() {
  runBtn.disabled = files.length === 0 || !workerReady;
  clearBtn.disabled = files.length === 0;
  statusEl.textContent = `${files.length}개 파일 대기 중 (전송 없음)`;
}

clearBtn.onclick = () => {
  files = [];
  last = [];
  afterFiles();
  rows.innerHTML = `<tr><td colspan="5" class="empty">아직 결과가 없습니다.</td></tr>`;
  diffs.textContent = "파일을 처리하면 문단 단위 diff가 표시됩니다.";
  diffs.classList.add("empty-box");
  fileDls.innerHTML = "";
  dlMasked.disabled = dlCsv.disabled = dlJson.disabled = dlHtml.disabled = true;
};

runBtn.onclick = async () => {
  if (!workerReady) return;
  runBtn.disabled = true;
  statusEl.textContent = "워커에서 탐지·마스킹 중…";
  try {
    const copies = files.map((f) => ({ name: f.name, data: f.data.slice(0) }));
    const msg = await call({
      type: "process",
      files: copies,
      extraRules: extraRules.value,
      maskMode: maskMode.value,
    });
    if (msg.type !== "result") throw new Error("unexpected worker message");
    last = msg.results;
    render(last);
    statusEl.textContent = `완료 — ${last.length}개 파일, 서버 전송 없음`;
  } catch (e) {
    statusEl.textContent = `오류: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    runBtn.disabled = files.length === 0;
  }
};

function render(results: ProcessResult[]) {
  rows.innerHTML = "";
  diffs.innerHTML = "";
  diffs.classList.remove("empty-box");
  fileDls.innerHTML = "";
  if (!results.length) {
    rows.innerHTML = `<tr><td colspan="5" class="empty">결과가 없습니다.</td></tr>`;
    return;
  }
  for (const r of results) {
    const report = r.report;
    if (report.warnings.length) {
      const wr = document.createElement("tr");
      wr.innerHTML = `<td>${esc(report.filename)}</td><td colspan="4">${warn(report)}</td>`;
      rows.appendChild(wr);
    }
    if (!report.summaries.length) {
      const tr = document.createElement("tr");
      tr.innerHTML = `<td>${esc(report.filename)}</td><td colspan="4">탐지 없음</td>`;
      rows.appendChild(tr);
    }
    for (const s of report.summaries) {
      const tr = document.createElement("tr");
      tr.innerHTML = `<td>${esc(report.filename)}</td>
        <td>${esc(s.label)}</td>
        <td>${s.count}</td>
        <td>확정 ${s.confirmed} / 의심 ${s.suspicious}${s.already_masked ? ` / 마스킹됨 ${s.already_masked}` : ""}</td>
        <td><code>${esc(s.preview)}</code></td>`;
      rows.appendChild(tr);
    }
    if (report.diffs.length) {
      const h = document.createElement("h3");
      h.textContent = report.filename;
      diffs.appendChild(h);
    }
    for (const d of report.diffs) {
      const box = document.createElement("div");
      box.className = "diff";
      box.innerHTML = `<div class="before">${esc(d.before)}</div><div class="after">${esc(d.after)}</div>`;
      diffs.appendChild(box);
    }
    const stem = report.filename.replace(/\.[^.]+$/, "");
    addDl(fileDls, `${stem}-report.csv`, () => downloadOneReport(r, "csv"));
    addDl(fileDls, `${stem}-report.json`, () => downloadOneReport(r, "json"));
    addDl(fileDls, `${stem}-report.html`, () => downloadOneReport(r, "html"));
    if (r.masked && r.outputName) {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = r.outputName + (r.fallbackNote ? " (대체)" : "");
      const bytes = r.masked;
      const name = r.outputName;
      b.onclick = () => downloadBlob(new Blob([blobPart(bytes)]), name);
      fileDls.appendChild(b);
    }
  }
  if (!diffs.childElementCount) {
    diffs.textContent = "변경된 문단이 없습니다.";
    diffs.classList.add("empty-box");
  }
  const anyMasked = results.some((r) => r.masked && r.outputName);
  dlMasked.disabled = !anyMasked;
  dlCsv.disabled = dlJson.disabled = dlHtml.disabled = false;
}

function warn(report: FileReport): string {
  return report.warnings.length ? ` <span class="warn">${esc(report.warnings.join(" · "))}</span>` : "";
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function downloadBlob(blob: Blob, name: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

async function downloadReports(kind: "csv" | "json" | "html") {
  const reportsJson = JSON.stringify(last.map((r) => r.report));
  const msg = await call({ type: "reports", reportsJson });
  if (msg.type !== "reports") return;
  const map = { csv: msg.csv, json: msg.json, html: msg.html };
  const mime = {
    csv: "text/csv;charset=utf-8",
    json: "application/json;charset=utf-8",
    html: "text/html;charset=utf-8",
  };
  downloadBlob(new Blob([map[kind]], { type: mime[kind] }), `pii-report.${kind}`);
}

dlCsv.onclick = () => downloadReports("csv");
dlJson.onclick = () => downloadReports("json");
dlHtml.onclick = () => downloadReports("html");

function addDl(parent: HTMLElement, label: string, fn: () => void) {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = label;
  b.onclick = fn;
  parent.appendChild(b);
}

async function downloadOneReport(r: ProcessResult, kind: "csv" | "json" | "html") {
  const reportsJson = JSON.stringify([r.report]);
  const msg = await call({ type: "reports", reportsJson });
  if (msg.type !== "reports") return;
  const map = { csv: msg.csv, json: msg.json, html: msg.html };
  const mime = {
    csv: "text/csv;charset=utf-8",
    json: "application/json;charset=utf-8",
    html: "text/html;charset=utf-8",
  };
  const stem = r.report.filename.replace(/\.[^.]+$/, "");
  downloadBlob(new Blob([map[kind]], { type: mime[kind] }), `${stem}-report.${kind}`);
}

dlMasked.onclick = async () => {
  const pack = last
    .filter((r) => r.masked && r.outputName)
    .map((r) => ({
      name: r.outputName as string,
      data: blobPart(r.masked as Uint8Array) as ArrayBuffer,
    }));
  const msg = await call({ type: "zip", files: pack });
  if (msg.type === "zip") {
    downloadBlob(new Blob([blobPart(msg.data)], { type: "application/zip" }), "masked-files.zip");
  }
};
