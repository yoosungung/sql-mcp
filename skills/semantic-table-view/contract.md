# table · view 계약

이 파일은 스킬 동봉 정본이다. 외부 레포 문서를 읽지 않는다.

## 역할

- **코딩 에이전트**가 `table` / `view` JSON을 작성한다.
- 참조 방향: `table → view → cube → metric|segment` (cube 이하는 다른 스킬).

## 이름

- **시맨틱 `name`:** `^[A-Za-z_][A-Za-z0-9_]*$` (문서 name · view 컬럼 · target 등)
- **물리 `physical_ident`:** 공백·기호 허용(따옴표·백틱·제어·`;` 금지) — `db`/`schema`/`table`, table 컬럼/`pk`/`fk` 컬럼, view `mappings.column`·join `on` 컬럼
- 파일로 둘 때 파일명(확장자 제외) = 문서 `name`
- **우선권:** cube · dimension · fact · metric · segment가 자연스러운 이름을 먼저 갖는다(검색 대상). 겹치면 table · view · view 컬럼 논리 이름에 `_table` / `_view` / `_col` 등을 붙인다. 물리 경로는 유지. 물리 컬럼명이 시맨틱 패턴 밖이면 view에서 별칭을 두고 mapping에 물리를 남긴다. dim/fact `name`과 그 `column`(view 컬럼)이 같으면 공유 허용.

## table

필수: `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter`.

| 필드 | 의미 |
|------|------|
| `db` / `schema` / `table` | 물리 DB·스키마·테이블 |
| `name` | 논리 테이블 이름(view·fk가 이 이름으로 참조) |
| `columns[]` | `name`, `type`, `nullable` (`as_code`는 dimension에 둠 — cube 스킬) |
| `pk` | 컬럼 이름 배열 |
| `fk[]` | `columns`(이 테이블) + `references.table`(다른 **논리** table `name`) + `references.columns`. **이름 규칙:** 컬럼 `{table}_id`(또는 CamelCase `{Table}Id`)가 있고 논리 테이블 `{table}`(대소문자 무시; 없으면 단순 복수 `{table}s`)에 `id`/`Id` 컬럼이 있으면 `fk`·join은 `{col} → {table}.id`로 둔다(자식 자신의 PK 컬럼은 제외). |
| `permission_filters[]` | `{ role, expression }` — 호출자 role과 같을 때 행 필터 |
| `scope_filter` / `default_filter` | `{ expression }` 또는 `null` |
| `role` | 생략 또는 `"all"` 또는 role 이름 배열 |

**필터 식은 물리 컬럼 이름**을 쓴다. 신뢰된 SQL 조건식이며 새 DSL이 아니다. 서브쿼리·다중 문장·쓰기·쓰기 CTE 금지.

컴파일 시 table 관련 필터는 다음 순서로 AND된다: `scope_filter` → role에 맞는 `permission_filters` → `default_filter` (이후 metric/segment filter는 상위 스킬).

## view

필수: `kind`, `name`, `combine`, `sources`, `columns`. cube는 view 하나에 연결되므로, 단일 물리 테이블 허브도 **identity view**(`combine: join`, source 1, `joins: []`)로 둔다.

| combine | sources | 추가 |
|---------|---------|------|
| `join` | table (≥1) | source 1개면 `joins: []`(identity). 2개 이상이면 `joins`(≥1): `left`, `right`, `type`(`inner`/`left`/`right`/`full`), `on[]`(`left_column`, `right_column`) |
| `union` / `minus` | table | 첫 source가 기준; 각 view 컬럼의 `mappings`가 **모든** source를 가리킴 |

요약·상세 후보는 view가 아니라 cube `choice`다.

`columns[]`: `name` + `mappings[]`(`source` = source 문서 `name`, `column` = 그 source의 물리 컬럼).

### Join 뷰 설계 및 프루닝 규칙 (`combine: "join"`)

1. **직접 FK 간선(사이클 포함) 전수 보존:**
   - 단일 BFS 스패닝 트리만 남기고 사이클을 닫는 유효 FK 간선(예: `orders ↔ customer`, `order_lines ↔ products`)을 버리지 않는다.
   - 서버 컴파일러(Step 4)가 질의된 컬럼의 필수 테이블 집합($T_{req}$) 간 **최단 경로(Steiner)**만 선택하고 사이클 없는 스패닝 트리 `JOIN`만 방출하므로, `view.joins`에는 테이블 쌍별 유효 FK 간선을 모두 넣어야 불필요한 허브 테이블 경유(Fan/Chasm Trap)가 발생하지 않는다.
   - 모든 조인이 `inner`이면 질의에 쓰이지 않은 `sources[0]`(루트 테이블)도 자동 프루닝된다.
2. **가짜 PK↔PK 조인 추론 금지 · `{table}_id`→`{table}.id` · 다중 FK 우선순위:**
   - 서로 다른 엔티티의 단일 PK 컬럼명이 우연히 같다고(`product.id`와 `warehouse.id`) 직접 조인하지 않는다. M:N은 연결 테이블(예: `stock(product_id, warehouse_id)`) 경유.
   - 물리/선언 FK가 없어도 컬럼 `{table}_id` 또는 `{Table}Id`가 있으면 논리 테이블 `{table}`(없으면 `{table}s`)의 `id`/`Id`와 조인한다. 예: `orders.customer_id → customer.id`, `comments.PostId → posts.Id`. 자식 테이블 자신의 PK이거나 부모에 `id` 컬럼이 없으면 적용하지 않는다.
   - 카탈로그 FK 목록 순서는 방언마다 다를 수 있다(역순·미지정). **테이블 컬럼 선언 순서**로 정렬한 뒤 쓴다.
   - 동일 테이블 쌍에 대체 키 FK가 여럿이면 **참조 부모 PK 여부 → 물리 컬럼명 일치(`customer_id = customer_id`) → 더 짧은 대표 키(`customer_id` < `customer_legacy_id`) → 컬럼 선언 순서**로 대표 간선을 선택한다.
3. **Role-Playing 소규모 룩업 테이블 분리:**
   - 한 자식 테이블이 룩업의 동일 키를 2~3개 역할 FK로 참조하면(예: `ship_to_address_id`/`bill_to_address_id`→`address.id`), **클론 table 문서를 만들지 말고** view `sources`에 동일 `table` + 서로 다른 `alias`로 이중 조인한다(예: `{ kind:"table", table:"address" }` · `{ kind:"table", table:"address", alias:"bill_to" }`).
   - 역할별 컬럼은 alias 접두로 노출되며, 질의에서 참조한 역할 조인만 프루닝으로 남긴다. 동일 부모 FK가 4개 초과(슬롯 배열 등)면 조인하지 않고 ID dimension만 둔다.
4. **조인 키 동치류(Equivalence Class) 다중 매핑:**
   - `on` 조건으로 묶인 조인 키 동치류에서 물리 컬럼명이 같으면(`customer_id`, `order_id`) 하나의 view 컬럼으로 묶고, 모든 참여 테이블의 `{ source, column }`을 `mappings`에 넣는다(PK 테이블 우선).
   - 조인 양끝의 물리 컬럼명이 다르면(`orders.order_id = shipments.ref_order`, `customer.postal_code = region.zip`), 오른쪽 PK 컬럼을 흡수해 없애지 말고 **양쪽 이름을 모두 view 컬럼으로 노출**하되(단, 비-루트 테이블의 범용 `"id"` 컬럼은 제외), 각 view 컬럼의 `mappings`에 자신의 테이블을 첫 번째(`mappings[0]`)로 두고 동치류의 상대 테이블 매핑을 뒤에 추가한다. 그래야 컴파일러가 어느 쪽 테이블만 단독 질의하더라도 불필요한 조인 없이 포함된 테이블의 컬럼으로 안전하게 바인딩한다.

## Quick checks

- [ ] `fk.references.table`은 존재(또는 함께 등록할) 논리 table `name` (등록 시 부모 table 먼저 생성하거나 `fk: []`로 선생성 후 update)
- [ ] `{table}_id`/`{Table}Id` 컬럼은 부모 `{table}.id`(또는 `{table}s.id`)로 `fk`·join 되어 있음(자체 PK 제외)
- [ ] view `sources[].table`이 카탈로그 table/view `name`과 일치하고, `alias`(생략 시=`table`)는 view 내 유일
- [ ] join `on` 컬럼이 각 source에 존재하며, 사이클을 닫는 직접 FK 조인 간선이 누락되지 않음
- [ ] 조인 키 컬럼의 `mappings`에 동치류 테이블들이 함께 매핑되어 단독 테이블 질의 시 조인 프루닝이 가능함
- [ ] union/minus: 모든 mapping이 전 source를 덮음
- [ ] 필터 식별자는 물리 컬럼
