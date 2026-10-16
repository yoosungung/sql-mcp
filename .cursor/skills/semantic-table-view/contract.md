# table · view 계약

이 파일은 스킬 동봉 정본이다. 외부 레포 문서를 읽지 않는다.

## 역할

- **코딩 에이전트**가 `table` / `view` JSON을 작성한다.
- 참조 방향: `table → view → cube → metric|segment` (cube 이하는 다른 스킬).

## 이름

- `name` 패턴: `^[A-Za-z_][A-Za-z0-9_]*$`
- 파일로 둘 때 파일명(확장자 제외) = `name`
- **우선권:** cube · dimension · fact · metric · segment가 자연스러운 이름을 먼저 갖는다(검색 대상). 겹치면 table · view · view 컬럼 논리 이름에 `_table` / `_view` / `_col` 등을 붙인다. 물리 `db`/`schema`/`table`·table 컬럼명(물리)은 유지. dim/fact `name`과 그 `column`(view 컬럼)이 같으면 공유 허용.

## table

필수: `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter`.

| 필드 | 의미 |
|------|------|
| `db` / `schema` / `table` | 물리 DB·스키마·테이블 |
| `name` | 논리 테이블 이름(view·fk가 이 이름으로 참조) |
| `columns[]` | `name`, `type`, `nullable`; 선택 `as_code` 코드사전 `{ table, key_column, value_column, where? }`(에이전트 기입 — 서버는 추론하지 않음) |
| `pk` | 컬럼 이름 배열 |
| `fk[]` | `columns`(이 테이블) + `references.table`(다른 **논리** table `name`) + `references.columns` |
| `permission_filters[]` | `{ role, expression }` — 호출자 role과 같을 때 행 필터 |
| `scope_filter` / `default_filter` | `{ expression }` 또는 `null` |
| `role` | 생략 또는 `"all"` 또는 role 이름 배열 |

**필터 식은 물리 컬럼 이름**을 쓴다. 신뢰된 SQL 조건식이며 새 DSL이 아니다. 서브쿼리·다중 문장·쓰기·쓰기 CTE 금지.

컴파일 시 table 관련 필터는 다음 순서로 AND된다: `scope_filter` → role에 맞는 `permission_filters` → `default_filter` (이후 metric/segment filter는 상위 스킬).

## view

필수: `kind`, `name`, `combine`, `sources`, `columns`. `join`/`union`/`minus`의 `sources`는 **테이블 2개 이상**. 단일 물리 테이블만 있으면 view를 만들지 않고 table만 둔다.

| combine | sources | 추가 |
|---------|---------|------|
| `join` | table (≥2) | `joins`(≥1): `left`, `right`, `type`(`inner`/`left`/`right`/`full`), `on[]`(`left_column`, `right_column`) |
| `union` / `minus` | table | 첫 source가 기준; 각 view 컬럼의 `mappings`가 **모든** source를 가리킴 |
| `select` | **view** | 선택 `prefer`; 질의에 필요한 컬럼을 모두 가진 source 중 `summary: true` view를 우선. 없으면 나머지. 어느 source도 덮지 못하면 거절 |

`columns[]`: `name` + `mappings[]`(`source` = source 문서 `name`, `column` = 그 source의 컬럼).

join/union/minus에서 이름·의미·코드가 같은 컬럼은 mappings를 묶어 하나의 view 컬럼으로 만든다. mappings가 둘 이상이면 가리키는 컬럼 이름이 같다. 이름 충돌 시 view 쪽 이름을 바꾼다.

## Quick checks

- [ ] `fk.references.table`은 존재(또는 함께 등록할) 논리 table `name`
- [ ] view `sources[].name`이 논리 table/view `name`과 일치
- [ ] join `on` 컬럼이 각 source에 존재
- [ ] union/minus: 모든 mapping이 전 source를 덮음
- [ ] 필터 식별자는 물리 컬럼
