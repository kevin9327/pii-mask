use crate::detect::preview_line;
use crate::types::{Confidence, FileReport, TypeSummary};

pub fn summaries(findings: &[crate::types::Finding]) -> Vec<TypeSummary> {
    let mut order: Vec<(String, String)> = Vec::new();
    for f in findings {
        if !order.iter().any(|(id, _)| id == &f.rule_id) {
            order.push((f.rule_id.clone(), f.label.clone()));
        }
    }
    order
        .into_iter()
        .map(|(id, label)| {
            let group: Vec<_> = findings.iter().filter(|f| f.rule_id == id).collect();
            let confirmed = group
                .iter()
                .filter(|f| f.confidence == Confidence::Confirmed)
                .count();
            let suspicious = group
                .iter()
                .filter(|f| f.confidence == Confidence::Suspicious)
                .count();
            let already_masked = group
                .iter()
                .filter(|f| f.confidence == Confidence::AlreadyMasked)
                .count();
            let preview = group
                .first()
                .map(|f| preview_line(f))
                .unwrap_or_default();
            TypeSummary {
                rule_id: id,
                label,
                count: group.len(),
                confirmed,
                suspicious,
                already_masked,
                preview,
            }
        })
        .collect()
}

pub fn csv_report(reports: &[FileReport]) -> String {
    let mut out = String::from(
        "filename,type,label,count,confirmed,suspicious,already_masked,residual_confirmed,residual_suspicious,preview\n",
    );
    for r in reports {
        if r.summaries.is_empty() {
            out.push_str(&format!(
                "{},,,,0,0,0,0,{},{},\n",
                csv_escape(&r.filename),
                r.residual_confirmed,
                r.residual_suspicious
            ));
            continue;
        }
        for s in &r.summaries {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_escape(&r.filename),
                csv_escape(&s.rule_id),
                csv_escape(&s.label),
                s.count,
                s.confirmed,
                s.suspicious,
                s.already_masked,
                r.residual_confirmed,
                r.residual_suspicious,
                csv_escape(&s.preview)
            ));
        }
    }
    out
}

pub fn json_report(reports: &[FileReport]) -> crate::error::Result<String> {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        generated_by: &'static str,
        client_only: bool,
        files: &'a [FileReport],
    }
    Ok(serde_json::to_string_pretty(&Envelope {
        generated_by: "pii-mask",
        client_only: true,
        files: reports,
    })
    .map_err(|e| crate::error::Error::msg(e.to_string()))?)
}

pub fn html_report(reports: &[FileReport]) -> String {
    let mut body = String::new();
    for r in reports {
        body.push_str(&format!(
            "<h2>{} <small>{}</small></h2>",
            esc(&r.filename),
            r.format.as_str()
        ));
        body.push_str(&format!(
            "<p>잔여 확정 {} · 잔여 의심 {} (마스킹본을 같은 엔진으로 재스캔)</p>",
            r.residual_confirmed, r.residual_suspicious
        ));
        if !r.warnings.is_empty() {
            body.push_str("<ul class=\"warn\">");
            for w in &r.warnings {
                body.push_str(&format!("<li>{}</li>", esc(w)));
            }
            body.push_str("</ul>");
        }
        body.push_str("<table><thead><tr><th>항목 유형</th><th>건수</th><th>확정/의심</th><th>미리보기</th></tr></thead><tbody>");
        for s in &r.summaries {
            body.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>확정 {} / 의심 {} / 마스킹됨 {}</td><td><code>{}</code></td></tr>",
                esc(&s.label),
                s.count,
                s.confirmed,
                s.suspicious,
                s.already_masked,
                esc(&s.preview)
            ));
        }
        if r.summaries.is_empty() {
            body.push_str("<tr><td colspan=\"4\">탐지 없음</td></tr>");
        }
        body.push_str("</tbody></table>");
        if !r.diffs.is_empty() {
            body.push_str("<h3>마스킹 전/후</h3>");
            for d in &r.diffs {
                body.push_str(&format!(
                    "<div class=\"diff\"><div class=\"before\">{}</div><div class=\"after\">{}</div></div>",
                    esc(&d.before),
                    esc(&d.after)
                ));
            }
        }
    }
    format!(
        r#"<!DOCTYPE html><html lang="ko"><head><meta charset="utf-8"><title>개인정보 탐지 리포트</title>
<style>
body{{font-family:system-ui,sans-serif;margin:24px;background:#0f1419;color:#e7ecf3}}
h2{{border-bottom:1px solid #2a3340;padding-bottom:8px}}
table{{border-collapse:collapse;width:100%;margin:12px 0}}
th,td{{border:1px solid #2a3340;padding:8px;text-align:left;vertical-align:top}}
th{{background:#1b2430}}
code{{background:#1b2430;padding:2px 4px}}
.warn{{color:#e8b849}}
.diff{{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin:8px 0}}
.before{{background:#3a1f1f;padding:8px;white-space:pre-wrap}}
.after{{background:#1f3a28;padding:8px;white-space:pre-wrap}}
.banner{{background:#16324d;padding:12px;border-radius:8px}}
</style></head><body>
<p class="banner">이 리포트는 브라우저에서만 생성되었습니다. 파일은 서버로 전송되지 않습니다.</p>
{body}
</body></html>"#
    )
}

fn csv_escape(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
