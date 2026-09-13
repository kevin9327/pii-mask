/**
 * Typings for wasm-bindgen output. `web/src/wasm` is generated and gitignored;
 * this module declaration lets the worker typecheck without that folder.
 * The `*` form is used so a generated sibling `.d.ts` (when present) wins.
 */
declare module "*pii_wasm.js" {
  export default function init(module_or_path?: unknown): Promise<unknown>;

  export interface JsProcessResult {
    readonly report_json: string;
    readonly masked?: Uint8Array;
    readonly output_name?: string;
    readonly fallback_note?: string;
  }

  export function process_bytes(
    name: string,
    data: Uint8Array,
    extra_rules: string,
    mask_mode: string,
  ): JsProcessResult;

  export function reports_from_json(reports_json: string): {
    csv: string;
    json: string;
    html: string;
  };

  export function zip_named_files(names: string[], payloads: Uint8Array[]): Uint8Array;
}
