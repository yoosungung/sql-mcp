# table · view 계약

이 파일은 스킬 동봉 정본이다. 외부 레포 문서를 읽지 않는다.

## 역할

- **코딩 에이전트**가 `table` / `view` JSON을 작성한다.
- 참조 방향: `table → view → cube → metric|segment` (cube 이하는 다른 스킬). 초안 직후 도메인 서술은 **`semantic-from-qa`**.

## 이름

- **시맨틱 `name`:** `^[A-Za-z_][A-Za-z0-9_]*$` (문서 name · view 컬럼 · target 등)
- **물리 `physical_ident`:** 공백·기호 허용(따옴표·백틱·제어·`;` 금지) — `db`/`schema`/`table`, table 컬럼/`pk`/`fk` 컬럼, view `mappings.column`·join `on` 컬럼
- 파일로 둘 때 파일명(확장자 제외) = 문서 `name`
- **우선권:** cube · dimension · fact · metric · segment가 자연스러운 이름을 먼저 갖는다(검색 대상). 겹치면 table · view · view 컬럼 논리 이름에 `_table` / `_view` / `_col` 등을 붙인다. 물리 경로는 유지. 물리 컬럼명이 시맨틱 패턴 밖이면 view에서 별칭을 두고 mapping에 물리를 남긴다. dim/fact `name`과 그 `column`(view 컬럼)이 같으면 공유 허용.
- **kind 간 동명:** `table`/`view`/`cube`는 kind가 다르면 같은 `name`을 쓸 수 있다(예: cube `superhero` + table `superhero`). 파일명이 충돌하면 outdir에서 kind별 하위 폴더를 쓰거나, table/view만 `_table`/`_view` 접미사를 붙인다.

## table

필수: `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter`.

| 필드 | 의미 |
|------|------|
| `db` / `schema` / `table` | 물리 DB·스키마·테이블 |
| `name` | 논리 테이블 이름(view·fk가 이 이름으로 참조) |
| `columns[]` | `name`, `type`, `nullable` (`as_code`는 dimension에 둠 — cube 스킬) |
| `pk` | 물리 스키마에 **선언된** PK 컬럼만. 없으면 `[]` |
| `fk[]` | 물리 스키마에 **선언된** FK만. `columns` + `references.table`(논리 table `name`) + `references.columns`. 스키마에 FK가 없으면 `[]` |
| `permission_filters[]` | `{ role, expression }` — 호출자 role과 같을 때 행 필터 |
| `scope_filter` / `default_filter` | `{ expression }` 또는 `null` |
| `role` | 생략 또는 `"all"` 또는 role 이름 배열 |

**PK/FK 추론 금지.** 컬럼명 패턴(`{table}_id`, `{Table}Id`)만으로 `pk`/`fk`·join을 만들지 않는다. 방언 카탈로그·`PRAGMA foreign_key_list`·`INFORMATION_SCHEMA` 등 **DW가 노출한 제약**만 옮긴다. 제약이 비어 있으면 `pk: []` / `fk: []`이고, view `joins`도 스키마 FK(또는 사용자가 명시한 조인) 없이 추측하지 않는다.

**필터 식은 물리 컬럼 이름**을 쓴다. 신뢰된 SQL 조건식이며 새 DSL이 아니다. 서브쿼리·다중 문장·쓰기·쓰기 CTE 금지.

컴파일 시 table 관련 필터는 다음 순서로 AND된다: `scope_filter` → role에 맞는 `permission_filters` → `default_filter` (이후 metric/segment filter는 상위 스킬).

## view

필수: `kind`, `name`, `target`, `combine`, `sources`, `columns`. cube는 view 하나에 연결되므로, 단일 물리 테이블 허브도 **identity view**(`combine: join`, source 1, `joins: []`)로 둔다.

| combine | sources | 추가 |
|---------|---------|------|
| `join` | table (≥1) | source 1개면 `joins: []`(identity). 2개 이상이면 `joins`(≥1): `left`, `right`, `type`(`inner`/`left`/`right`/`full`), **`relationship`**, `on[]`(`left_column`, `right_column`, 선택 `op` 생략=`=`) |
| `union` / `minus` | table | 첫 source가 기준; 각 view 컬럼의 `mappings`가 **모든** source를 가리킴 |

요약·상세 후보는 view가 아니라 cube `choice`다.

`columns[]`: `name` + `mappings[]`(`source` = source 문서 `name` 또는 alias, `column` = 그 source의 물리 컬럼).

### Join 뷰 설계 및 프루닝 규칙 (`combine: "join"`)

1. **스키마 FK 간선만 사용 (사이클 포함 전수):**
   - view `joins`는 물리 스키마에 선언된 FK(및 동일 부모로의 다중 FK)에 대응한다. 이름 패턴으로 간선을 **추가 추론하지 않는다**.
   - 서버 컴파일러(Step 4)가 질의된 컬럼의 필수 테이블 집합($T_{req}$) 간 **최단 경로(Steiner)**만 선택하므로, 스키마에 있는 직접 FK 간선은 `view.joins`에 빠뜨리지 않는다.
   - **기본 `type`은 `inner`.** nullable FK로 행 보존이 필요할 때만 `left`(사용자·도메인이 명시한 경우).
   - 모든 조인이 `inner`이면 질의에 쓰이지 않은 `sources[0]`(루트 테이블)도 자동 프루닝된다.
2. **`relationship` (필수에 가깝게):**
   - 각 join에 `relationship`: `many_to_one` | `one_to_one` | `one_to_many` (SQL `type`과 별개, D29).
   - 허브→룩업·부모 PK = 보통 `many_to_one` 또는 `one_to_one`. 허브→자식/브릿지(행 복제) = `one_to_many`.
   - 비우면 그레인 가드가 그 조인을 보지 않는다. 에이전트 표면용 초안에서도 카디널리티를 적어 둔다.
3. **가짜 PK↔PK 조인 금지 · 다중 FK(스키마) 우선순위:**
   - 서로 다른 엔티티의 단일 PK 컬럼명이 우연히 같다고 직접 조인하지 않는다. M:N은 연결 테이블 경유(스키마 FK가 있을 때).
   - 동일 테이블 쌍에 스키마 FK가 여럿이면 **참조 부모 PK 여부 → 물리 컬럼명 일치 → 더 짧은 키 → 컬럼 선언 순서**로 대표 간선을 고르되, **역할이 다른 FK는 모두** role-play로 남긴다.
   - 카탈로그 FK 목록 순서는 방언마다 다를 수 있다. **테이블 컬럼 선언 순서**로 정렬한 뒤 쓴다.
4. **Role-Playing (스키마에 동일 부모 FK가 2~3개일 때):**
   - 예: `eye_colour_id`/`hair_colour_id`/`skin_colour_id`→`colour.id`가 **스키마 FK로** 있으면, 클론 table 없이 view `sources`에 동일 `table` + 서로 다른 `alias`.
   - 역할별 라벨 컬럼은 alias 접두로 노출. 동일 부모 FK가 4개 초과면 조인하지 않고 ID만 둘 수 있다.
5. **컬럼 노출 (검색·필터 표면):**
   - **라벨·측정값 우선:** 룩업의 표시 컬럼(`publisher_name`, `alignment`, `power_name` 등), 허브 자연키/이름, fact 수치.
   - **raw FK/PK id**는 허브 식별에 필요한 최소만(예: `hero_id` 또는 `id`). 모든 `*_id`를 view 컬럼으로 나열하지 않는다.
   - 조인 키 동치류: prune에 필요하면 `mappings`에 여러 source를 넣되, **바깥 `name`은 에이전트가 쓸 라벨/허브 키**로 둔다. 비-루트 범용 `"id"`를 별도 컬럼으로 노출하지 않아도 된다.
6. **허브 우선 `sources` 순서:**
   - `joins`가 있으면 `sources[0]`은 조인 그래프의 **허브**(차수가 가장 높은 중심 테이블). 그 다음 말단·룩업.
   - 공유 조인 키의 `mappings[0]`도 허브. identity view는 source 1개가 허브다.
7. **표면 모드 (에이전트 UX vs 그레인):**
   - **agent-surface (기본 초안):** 타깃당 **질의 표면 view(+cube) 하나**를 우선한다. 스키마 FK로 연결된 룩업·브릿지를 한 join view에 넣어도 된다. 자식/브릿지 조인에는 `relationship: "one_to_many"`를 반드시 적는다. (`SQL_MCP_GRAIN_GUARD`가 꺼진 전제.)
   - **grain-split:** `SQL_MCP_GRAIN_GUARD`가 켜졌거나 eval이 팬/캐즘 FAIL을 내면, 형제 팩트·브릿지를 **허브별 view/cube로 분리**한다 — [semantic-from-eval](../semantic-from-eval/methodology.md).

## Quick checks

- [ ] `pk`/`fk`는 물리 스키마 제약만 (이름 패턴 추론 없음). 없으면 `[]`
- [ ] `fk.references.table`은 존재(또는 함께 등록할) 논리 table `name`
- [ ] view `joins`의 각 간선이 스키마 FK(또는 사용자 명시)에 대응하고, `relationship`이 채워짐
- [ ] 기본 join `type`이 `inner`(예외만 `left` 등)
- [ ] view `sources[].table`이 카탈로그 table `name`과 일치하고, `alias`(생략 시=`table`)는 view 내 유일
- [ ] join `on` 컬럼이 각 source에 존재
- [ ] 노출 컬럼이 라벨·측정 위주(불필요한 raw `*_id` 남발 없음)
- [ ] `joins`가 있으면 `sources[0]`·공유 키 `mappings[0]`이 허브
- [ ] union/minus: 모든 mapping이 전 source를 덮음
- [ ] 필터 식별자는 물리 컬럼
