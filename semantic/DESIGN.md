# semantic

코딩 에이전트가 등록하는 JSON의 스키마 정의와 예시다. 이 디렉터리에는 실행 코드를 두지 않는다.

필드 의미와 연결 규칙은 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다. 컴파일 파이프라인은 [server/spec.md](../server/spec.md).

- `schema/document.schema.json` — 등록 JSON Schema 정본(교환·검증). 한 파일이다.
- `examples/` — 종류별 예시. 파일명(확장자 제외)은 `name`과 같다.
- `libsql/` — MCP 시맨틱 DB(libSQL) DDL.

## libSQL 카탈로그

`libsql/catalog.sql`은 MCP 메타 CRUD가 쓰는 시맨틱 DB다. 타깃 DW 물리 테이블이 아니다. `SemanticStore::open`이 매번 적용하므로 DDL은 `IF NOT EXISTS`로 멱등하다.

**저장 계층은 타입별 `meta_*` 테이블**이다(`meta_tables`, `meta_views`, `meta_cubes`, `meta_metrics`, `meta_segments` 및 자식 테이블). `document(kind,name,body)` blob은 쓰지 않는다.

`metadata_*` **입출력은 JSON**이다. create/update 시 schema·연결 검증 후 단일 Write 트랜잭션으로 타입별 행에 분해하고, get/list 시 JSON으로 재조립한다.

## 검색 인덱스 (D12)

JSON 교환 정본은 `schema/document.schema.json`이다. `column_index` / `column_index_fts`는 재조립된 등록 JSON을 펼친 **투영**이다. keyword는 **FTS5**이며 순위는 `bm25()`다(별도 BM25 엔진 아님). `embedding` blob으로 vector/hybrid(RRF)를 유지한다.

**검색 대상은 cube · dimension · fact · metric · segment만.** table · view · table 컬럼은 인덱싱하지 않는다. 투영 필드는 **이름·설명**이 기본이고, **dimension·fact만 `type`을 추가**한다(그 외 expression·filter·as_code·role·aggregation 등은 넣지 않음 — 노이즈·성능).

| field_role | 출처 | `text` |
|------------|------|--------|
| `document` | cube / metric / segment의 `name` + `description`(있으면) | `{name}` 또는 `{name} {description}` |
| `dimension` | cube `dimensions[]` | `{name} {type}` (`code`/`date`/…) |
| `fact` | cube `facts[]` | `{name} {type}` (`amount`/`fx`/`quantity`) |
| `metric` | metric 문서 | `{name}` 또는 `{name} {description}`, `field_role=metric` |
| `segment` | segment 문서 | `{name}` 또는 `{name} {description}`, `field_role=segment` |

FTS에 넣기 전 텍스트에 **한글 등 non-ASCII 구간의 문자 bigram**을 덧붙인다(D14 BM25 강화). CRUD 성공 후 인덱스를 전량 재구축한다. `embedding`은 D14 API가 있을 때만 채운 f32 blob(없으면 NULL).
