# cube · metric · segment 계약

이 파일은 스킬 동봉 정본이다.

## 선행

대상 `view`(및 그 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

참조 방향: `table → view → cube → metric|segment`.

## 이름·설명 (메타 사용 표면)

`name`·`description`은 **메타 사용**(DW 에이전트 검색·동의어·업무 의미)용이다. 메타 **생성** 메모(`from table.column`, `Auto cube over view…`, `Dimension X from Y` 등)를 넣지 않는다. 사용 서술이 없으면 `description`은 빈 문자열(`""`)이 낫다.

자연스러운 `name`은 **cube · dimension · fact · metric · segment** 우선(검색 대상: 이름·설명, dim/fact는 type 추가). 중복 시 table · view · view 컬럼이 접미사를 받아 양보한다(물리 경로·물리 컬럼명은 유지). dim/fact가 가리키는 view 컬럼과 멤버명이 같으면 OK.

## 식·역할 공통

- 필터·metric·segment·dimension/fact `expression`은 코딩 에이전트가 작성한 **신뢰된 SQL 조각**. 새 DSL 없음.
- **허용 함수 및 구문:**
  - 기본 집계(`SUM`, `COUNT`, `AVG`, `MIN`, `MAX`) 및 산술·비교·논리·`CASE WHEN`·`CAST`·`COALESCE`/`NULLIF`
  - 비율·산술: 표준 연산자 `+ - * /`를 1급으로 사용(`(A - B) * 1.0 / C`, `A / C`). emit이 `/` 분모를 `NULLIF(…, 0)`으로 감싸고 실수로 승격한다. `DIVIDE(num, den)`은 동일 emit의 호환 별칭.
  - 날짜·문자열 편의 스칼라 함수: `CURRENT_DATE`, `CURRENT_TIMESTAMP`, `NOW()`, `date_trunc('year'|'month'|'day'|'hour', x)`, `YEAR(x)`, `MONTH(x)`, `DAY(x)`, `DATEDIFF(end, start)`(일수), `EXTRACT(YEAR|MONTH|DAY|HOUR FROM x)`, `STRFTIME(fmt, x)`, `SUBSTR`/`SUBSTRING`, `LENGTH`, `REPLACE`, `INSTR`, `IFNULL`, `IIF(cond, t, f)`
  - **윈도우 함수 (`OVER (PARTITION BY … ORDER BY …)`):** `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `NTILE`, `PERCENT_RANK`, `CUME_DIST`, `LAG`, `LEAD`, `FIRST_VALUE`, `LAST_VALUE`, `NTH_VALUE` 및 `SUM`/`COUNT`/`AVG`/`MIN`/`MAX` `OVER (...)`
  - **읽기 전용 서브쿼리:** 등록 `metric.expression`·`metric.filter`·`segment.filter`와, DW `execute_sql`의 ad-hoc SELECT/WHERE(D26) 모두 `(SELECT …)`·`IN (SELECT …)`·`EXISTS (SELECT …)`를 허용한다. 서브쿼리 안에 cube가 있으면 재귀 컴파일하고, 실패하면 fail-closed. 외부 스코프의 식별자 치환은 서브쿼리 내부 스코프를 침범하지 않는다.
  - 다중 문장·쓰기·쓰기 CTE 거부.
- cube/metric/segment 식의 식별자: 저장 시에는 연결 그래프 위주; **컴파일 직전**에 dimension·fact·(해당 시) view/물리 컬럼으로 해석. 실패 시 `validation`.
- cube·metric·segment의 식에서 쓰는 이름은 그 cube의 **dimension / fact 이름**(metric `expression`의 피연산은 view 컬럼 기준 연산으로 작성하되, 질의·segment filter는 멤버 이름).
- `role` 생략 또는 `"all"` 또는 role 이름 배열.
- `encryption: true`인 dimension/fact는 암·복호화하지 않고 응답·샘플·로그에서 원문 마스킹(`null` 또는 `"***"`). AES 등은 타깃 DW 책임.

## cube

필수: `kind`, `name`, `target`, `view`, `description`, `dimensions`, `facts`.

`view`는 정본(상세) view 문서 **하나**. 요약 view가 있으면 선택 `choice[]`(`{ view, summary: true }`)와 `prefer: "summary"`를 함께 둔다. choice view의 `target`은 cube와 같고, 이름은 `cube.view`와 달라야 한다. 질의 컬럼 커버리지는 컴파일 때 검사한다.

**dimension**

- `description` 필드 필수, **내용**은 메타 사용용(검색·동의어·업무 의미). 없으면 `""`. 생성 출처(`from {table}.{column}` 등) 금지. 동명 컬럼 구분은 `name` 접두·view 매핑으로 하고 description에 물리 출처를 쓰지 않는다.
- `type`: `id` | `code` | `date` | `time` | `datetime` | `string` (`id`=식별 키, `code`=코드값)
- `column`(view 컬럼) **또는** `expression`(view 컬럼명을 쓰는 **스칼라**, 집계 금지 D28) — 둘 중 하나만
- `encryption`, `role`
- 선택 `as_code`: `{ table, key_column, value_column, where? }` 코드사전(집계 후 바깥에서 `LEFT JOIN`하여 `{dim}__value` 컬럼을 **추가 프로젝션**함; 주의: `{dim}` 조회 시 반환 컬럼 수가 늘어나며 WHERE에서 `{dim}__value` 직접 필터링은 불가하므로, 값 자체로 WHERE 필터링하거나 반환 컬럼 수를 유지해야 하는 룩업은 `view.joins`의 룩업/역할 테이블 조인으로 노출하는 것이 유리함)

**fact**

- `description` 필드 필수, **내용**은 메타 사용용(단위·업무 의미). 없으면 `""`. 생성 출처 금지.
- `type`: `amount` | `fx` | `quantity` (취급 방식 — dim `type`과 같은 축)
- `column` XOR `expression` (`expression`은 스칼라만, D28)
- `additive`: 기본 `true`. `false`는 행 그레인 비율 등 비가산 — `SUM`/`AVG`/`COUNT(col)` 거절, ungrouped `SELECT`/`WHERE`/`ORDER BY`/`MIN`/`MAX` 허용
- `encryption`, `role`

`expression`으로 만든 dimension/fact는 **저장 시** view 컬럼 연결 검사를 하지 않는다(식 식별자는 컴파일 단계).

## metric

필수: `kind`, `name`, `description`, `cube`, `expression`, `aggregation`, `filter`.

| 필드 | 규칙 |
|------|------|
| `description` | 메타 사용(검색·업무 의미). 없으면 `""` |
| `expression` | 연결된 cube의 view 컬럼(또는 fact/dim) 연산·집계·윈도우·서브쿼리 식. 예: `sum(sale_amount)` |
| `aggregation` | `group` \| `rollup` \| `cube` \| `grouping_sets` |
| `group_by` | `aggregation=group`일 때 필수: 명명 metric을 컴파일할 때 축이 없으면 쓰는 **기본 GROUP BY 축**(view 컬럼 배열, D24). 허용목록이 아니다. `execute_sql`은 metric 이름을 쓰지 않으므로(D26) **기본으로 쓰고 싶은 축만** 넣는다. |
| `filter` | `{ expression }` 또는 `null` |
| `name` | 같은 cube의 dimension·fact 이름과 **겹치면 안 됨**(행 비율 fact와 그룹 KPI metric은 다른 이름) |

## segment

필수: `kind`, `name`, `description`, `cube`, `filter`. `filter`는 비울 수 없음. 식은 cube 멤버 이름(단순 boolean 비교 및 `IN (SELECT …)`/`EXISTS (SELECT …)` 등 서브쿼리 허용). 예: `status_code = 'PAID'`.

## Quick checks

- [ ] `cube.view` 존재; `column` ∈ 정본 view columns
- [ ] `choice`와 `prefer`는 함께; choice view는 다른 join/union/minus view, 같은 target
- [ ] cube·dim·fact·metric·segment `description`은 메타 사용 서술만(생성 출처 문구 없음). 없으면 `""`
- [ ] `column`과 `expression` 동시 사용 없음
- [ ] metric `name` ∉ 같은 cube dimension/fact names
- [ ] `aggregation=group` ⇒ `group_by` ⊆ view columns (기본 리포트 축만; 차원 전부 나열 금지에 가깝게)
- [ ] segment `filter.expression` non-empty
- [ ] 식에 다중 문장·쓰기·쓰기 CTE 없음
- [ ] `as_code.table`·key/value 컬럼이 등록 table에 존재(dimension만)
