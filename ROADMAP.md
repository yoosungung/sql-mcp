# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**.  
Frontend / Admin API / **세션형 Streamable HTTP**는 **취소**. Transport는 **일반 HTTP**(기본) + **stdio**(로컬).

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 시맨틱 DB | **MCP 프로세스에 임베드** (원격 시맨틱 서버 없음) |
| 시맨틱 엔진 | **libSQL** (임베디드) — 네이티브 벡터·FTS, 파일 하나로 M1 CRUD·이후 검색에 적합 |
| 시맨틱 등록 문서 | `table` · `view` · `cube` · `metric` · `segment`. 스키마·예시는 `semantic/` |
| 조회 대상 | **타깃 DB**, **복수** 허용 |
| 타깃 SQL | SELECT only |
| 메타 | MCP `metadata_*` CRUD, **JSON 바디**. 검증은 `semantic/schema/document.schema.json` |
| 컬럼 검색 | BM25/벡터/hybrid — 인덱스는 등록 JSON 투영(D12). 임베딩=외부 API, 없으면 강화 BM25(D14) |
| MCP 툴 | `list_targets`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`, `metadata_{create,get,list,update,delete}` |
| Transport | **일반 HTTP**(기본) + **stdio**(로컬). 세션형 Streamable HTTP 취소 |
| Frontend / Admin API | 취소 |
| 개발 | TDD, 문서 우선 |
| metric·segment 식 (D16) | 새 DSL 없음 · `sqlparser` + AST 제한 |
| `encryption` (D17) | 응답 마스킹(암·복호화 아님) |
| 식 식별자 검사 (D18) | 저장=연결 그래프 · 컴파일=AST 식별자 |

## 시맨틱=libSQL 선택 이유

임베디드 + (장차) BM25/벡터 + 지금 당장 JSON 메타 CRUD에 **libSQL이 잘 맞음**.

- 확장(`sqlite-vec`) 없이 벡터·FTS 경로가 엔진에 있음
- 프로세스 내 파일 DB → `rmcp` 안에 심기 자연스러움
- 타입별 `meta_*` 카탈로그 + JSON CRUD 통로·검색 인덱스 투영이 가능
- 주의: hybrid는 앱 RRF. Turso 브랜드/API 이동은 있으나 M1 시맨틱은 `libsql` 핀(D2)
- 타깃 DB는 libSQL일 필요 없음(Postgres 등 별도, D13)

**임베딩 공급(D14):** 외부 OpenAI-호환 HTTP API. 벡터는 libSQL에 저장. API 없으면 **강화 BM25(keyword)만** — HashEmbedder·로컬 ONNX는 쓰지 않음.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 임베디드 시맨틱 엔진 | **결정: libSQL** |
| D2 | 클라이언트 | **결정:** 시맨틱=`libsql` 0.9 (`core`만) · 타깃=`sqlx` 0.9(+postgres). **크레이트 분리**. Rust **1.94+** (`rust-toolchain.toml`) |
| D14 | 임베딩·검색 폴백 | **결정: 외부 API만 임베딩.** `SQL_MCP_EMBEDDING_URL`+`API_KEY` → vector/hybrid. **미설정 시 keyword(BM25)만**(vector/hybrid 요청도 keyword로 폴백). 오프라인 품질은 **BM25 강화**(한글 유지·문자 bigram·`as_code` 등 인덱스 텍스트). HashEmbedder=테스트 전용. 로컬 ONNX 비채택 |
| D7 | 배포 | **결정(M3): release 바이너리 + 컨테이너 이미지**. Kustomize 베이스는 `deploy/k8s/base/` |
| D9 | code 판정 | **결정:** `sample_column`의 `is_code`는 런타임 휴리스틱(컬럼명·샘플). **`as_code` 메타는 코딩 에이전트가 등록 JSON에 기입**하고 MCP는 **준 그대로 저장만**(추론·덮어쓰기 없음) |
| D12 | 등록 스키마 ↔ 저장·검색 | **결정: JSON 교환=`document.schema.json` · 저장=타입별 `meta_*` · 검색 투영=`column_index`+FTS5(`bm25`)+embedding(hybrid RRF). 규칙은 `semantic/DESIGN.md`·`server/spec.md`** |
| D13 | 타깃 엔진 | **결정: Postgres(`sqlx`) + SQLite(`libsql` 로컬)**. sqlx에 `sqlite` feature를 넣지 않는다(번들 SQLite 이중 링크 방지). 테스트 타깃은 BIRD Mini-Dev SQLite (`target_db/`) |
| D15 | `metadata_update` | **결정: 전체 replace** (merge-patch 아님). `body`는 문서 JSON 전체 |
| D16 | metric·segment 식 문법 | **결정: 새 DSL 없음.** `sqlparser`로 파싱한 **신뢰된 SQL 조각** + AST 제한. metric `expression` = 스칼라/집계식만 · segment·filter = 단일 boolean 식만. 서브쿼리·다중 문장·쓰기·CTE 거부. 식별자: table 필터=물리 컬럼 · cube/metric/segment=dimension·fact 이름 |
| D17 | `encryption: true` | **결정: 암·복호화 아님 → 응답 마스킹.** SELECT/샘플/로그에서 해당 셀을 `null`(또는 `"***"`)로 치환. AES·토큰화는 타깃 DW 책임 |
| D18 | expression/filter 컬럼 검사 시점 | **결정: 이중 검사.** `metadata_*` = 연결 그래프 · 컴파일=AST 식별자. `execute_sql`은 시맨틱 컴파일 필수 |
| D19 | `execute_sql` 입력 | **결정: 시맨틱 SELECT만.** 툴명 유지. 자유 물리 SELECT 경로 없음 |

(제거: D5 세션형 Streamable HTTP, D10/D11 Frontend. stdio는 로컬용으로 복원)

## 마일스톤

### M0 — 문서·계약

- [x] MCP-only · 임베디드 libSQL · 복수 타깃 · 메타 CRUD(JSON) · **일반 HTTP** · **stdio(로컬)**
- [x] 시맨틱 등록 JSON Schema·예시·카탈로그 DDL (`semantic/`)
- [x] D2·D13·D15 합의 (D14는 M2)

### M1 — 타깃 조회 + 메타 CRUD (일반 HTTP)

1. [x] `server/` + **libSQL** 임베디드 + HTTP 리스너 + `DESIGN.md` / Commands
2. [x] `metadata_*` CRUD — `document.schema.json` 검증 + `document` 테이블 저장 · update=전체 replace(D15)
3. [x] `list_targets` · 타깃 SELECT 가드 · `list_tables` / `describe_table` / `execute_sql` / `sample_column` · 엔진 Postgres·SQLite(D13)
4. [x] `search_columns`는 스텁 또는 no-op 허용 — 인덱스 본구현은 M2
5. [x] HTTP 클라이언트로 스모크 (`cargo test -p sql-mcp`)

**완료 기준:** 타깃 SELECT·탐색 + 메타 JSON CRUD가 **일반 HTTP**로 동작. (Postgres 실연결 스모크는 `http_smoke` ignore 테스트·DSN 환경 선택)

### M2 — 검색·컴파일

- [x] `search_columns` BM25(FTS5) / 벡터 / hybrid(RRF) · D14(API 또는 강화 BM25) · D9 휴리스틱
- [x] 메타 CRUD 시 `column_index` 재구축
- [x] table→물리 SQL 컴파일(필터 AND) + SELECT AST 가드 유지
- [x] CI (`.github/workflows/ci.yml`)
- [x] OpenAI-호환 임베딩 HTTP 클라이언트 배선 (`SQL_MCP_EMBEDDING_*`, 미설정 시 BM25만 · D14)
- [x] cube/metric/segment → 물리 SQL 컴파일 (`compile_metric_select`)
- [x] D16 식 AST 제한 · D18 컴파일 식별자 검사 · D17 encryption 마스킹
- [x] 타깃 엔진 SQLite (`libsql`) · BIRD Mini-Dev → `target_db/`

### M3 — 배포·하드닝

- [x] (D7) release 바이너리 + 컨테이너 · `deploy/SETUP.md` · `deploy/k8s/base/`
- [x] `/health` 프로브 · tracing 로그
- [x] D16–D18 본구현 (`expr_guard` · `mask`)
- [x] MCP Resource로 `document.schema.json` 제공 (`resources/list` · `resources/read`)
- [ ] 타깃 엔진 추가 확장·운영 하드닝(인증·rate limit 등)은 필요 시 후속

### M4 — 시맨틱 컴파일러 · 타입별 카탈로그

- [x] 타입별 `meta_*` 저장 + `metadata_*` JSON 분해/재조립
- [x] `execute_sql` = 시맨틱 SQL → `pipeline` → 물리 SELECT (자유 물리 SELECT 삭제, D19)
- [x] keyword = FTS5/`bm25` · vector/hybrid 유지 (D12)
- [x] 요약 뷰 라우팅 (`combine=select` + `prefer=summary`) · join Steiner 근사 pruning · 인라인 뷰 필터 푸시다운 · encryption 프로젝션 NULL
- [x] union/minus 컴파일 · union pruning(`1=0` scope · 미매핑 source 제거)
- [x] metric/view 식별자 치환을 sqlparser `VisitorMut` AST 경로로 이관 (`ast_rewrite`)
- [x] metric SELECT AST 조립(`ast_select`) · 날짜 리터럴 구간 union disjoint(`date_range`)
- [x] bind-param 구간 비교(`params` → `:name` 치환) · OR(DNF) 날짜식 · `execute_sql.params`

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 등록 스키마 → `semantic/` · 서버 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
