# PII Mask — 브라우저 전용 개인정보 탐지·마스킹

저장소: https://github.com/kevin9327/pii-mask

모든 처리는 클라이언트에서만 수행됩니다. 파일은 어떤 서버로도 전송되지 않습니다.

## 구조

- `pii-core` — 파싱 / 탐지 / 마스킹 순수 Rust 로직
- `pii-wasm` — wasm-bindgen 바인딩
- `web` — Vite + TypeScript (프레임워크 없음). 무거운 작업은 Web Worker

HWP/HWPX/HWP3 는 [docagent](https://github.com/kevin9327/docagent) 코덱(`docagent-hwp5`, `docagent-hwpx`, `docagent-hwp3`)으로 텍스트·단락을 추출하고, 같은 IR 을 써서 마스킹본을 다시 씁니다. rhwp 는 사용하지 않습니다.

## 실행

```bash
# 코어 테스트
cargo test -p pii-core

# 웹 (wasm-bindgen-cli 필요)
cd web
npm install
npm run dev
```

브라우저에서 `http://127.0.0.1:4177` 을 엽니다.

## 지원 확장자 (process_file 왕복 검증)

| 확장자 | 포맷 | 마스킹 출력 |
|---|---|---|
| `.hwp` / `.HWP` | HWP 5 (docagent-hwp5) | 원 포맷 |
| `.hwpx` / `.HWPX` | HWPX (docagent-hwpx) | 원 포맷 |
| `.hwp3` | HWP 3 (docagent-hwp3) | 원 포맷 |
| `.txt` `.text` `.md` `.log` | 텍스트 | 원 포맷 |
| `.csv` `.json` | CSV / JSON | 원 포맷 |
| `.pdf` | PDF 텍스트 레이어 | TXT 대체 + 안내 |
| `.docx` `.xlsx` | ZIP+XML | 원 포맷 |

스캔 PDF는 "텍스트 없음". PPTX 등 알 수 없는 ZIP은 지원하지 않음으로 리포트하고 원본을 유지합니다.

## 탐지 규칙

기본 규칙은 `pii-core/rules.toml` 입니다. UI의 텍스트 영역에 `[[rules]]` 를 추가하면 기관 커스텀 패턴을 덮어쓰거나 더할 수 있습니다.
