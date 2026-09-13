use wasm_bindgen::prelude::*;

use pii_core::report::{csv_report, html_report, json_report};
use pii_core::{
    process_file, zip_files, FileReport, MaskMode, ProcessConfig, RuleSet, DEFAULT_RULES_TOML,
};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn default_rules() -> String {
    DEFAULT_RULES_TOML.to_string()
}

#[wasm_bindgen]
pub struct JsProcessResult {
    report_json: String,
    masked: Option<Vec<u8>>,
    output_name: Option<String>,
    fallback_note: Option<String>,
}

#[wasm_bindgen]
impl JsProcessResult {
    #[wasm_bindgen(getter)]
    pub fn report_json(&self) -> String {
        self.report_json.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn masked(&self) -> Option<Vec<u8>> {
        self.masked.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn output_name(&self) -> Option<String> {
        self.output_name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn fallback_note(&self) -> Option<String> {
        self.fallback_note.clone()
    }
}

fn make_config(extra_rules: &str, mask_mode: &str) -> Result<ProcessConfig, JsValue> {
    let rules = RuleSet::builtin()
        .and_then(|r| r.with_extra(extra_rules))
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(ProcessConfig {
        rules,
        mask_mode: MaskMode::parse(mask_mode),
        do_mask: true,
    })
}

#[wasm_bindgen]
pub fn process_bytes(
    name: &str,
    data: &[u8],
    extra_rules: &str,
    mask_mode: &str,
) -> Result<JsProcessResult, JsValue> {
    let cfg = make_config(extra_rules, mask_mode)?;
    let out = process_file(name, data, &cfg).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let report_json = serde_json::to_string(&out.report)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(JsProcessResult {
        report_json,
        masked: out.masked_bytes,
        output_name: out.output_filename,
        fallback_note: out.fallback_note,
    })
}

#[wasm_bindgen]
pub fn reports_from_json(reports_json: &str) -> Result<JsValue, JsValue> {
    let reports: Vec<FileReport> = serde_json::from_str(reports_json)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let csv = csv_report(&reports);
    let json = json_report(&reports).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let html = html_report(&reports);
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"csv".into(), &csv.into())?;
    js_sys::Reflect::set(&obj, &"json".into(), &json.into())?;
    js_sys::Reflect::set(&obj, &"html".into(), &html.into())?;
    Ok(obj.into())
}

#[wasm_bindgen]
pub fn zip_named_files(names: js_sys::Array, payloads: js_sys::Array) -> Result<Vec<u8>, JsValue> {
    if names.length() != payloads.length() {
        return Err(JsValue::from_str("names/payloads length mismatch"));
    }
    let mut files = Vec::new();
    for i in 0..names.length() {
        let name = names
            .get(i)
            .as_string()
            .ok_or_else(|| JsValue::from_str("name must be a string"))?;
        let bytes = js_sys::Uint8Array::new(&payloads.get(i)).to_vec();
        files.push((name, bytes));
    }
    zip_files(&files).map_err(|e| JsValue::from_str(&e.to_string()))
}
