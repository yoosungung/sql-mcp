---
name: semantic-cube-metric-segment
description: >-
  Designs semantic cube, metric, and segment registration JSON for sql-mcp on
  top of an existing view. Use when defining dimensions, facts, KPIs, cohort
  filters, or segments after table/view docs exist.
---

# Semantic cube · metric · segment

선행: 대상 `view`(및 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

규칙: [contract.md](contract.md). 예시: [examples/](examples/).

## Output layout

`{outdir}/{name}.json` (table/view와 같은 디렉터리에 평탄하게). view가 많으면 휴리스틱 초안 스크립트 허용 — dimension/fact 분류는 [contract.md](contract.md) 준수 후 검토. **자명한 metric/segment 대량 생성 금지.** **MCP 등록은 사용자 요청 전 금지.**

## Workflow

```
Progress:
- [ ] 1. view 컬럼 목록 확보 (초안 파일 또는 GET /api/meta)
- [ ] 2. cube: 기본 name=target · description("") 가능 · dimensions/facts · grain은 GRAIN_GUARD 시에만
- [ ] 3. metric/segment: 이 단계에서 휴리스틱 남발 금지 — 골격만 필요하면 최소
- [ ] 4. 이름 충돌 검사
- [ ] 5. 필수 handoff → semantic-from-qa (description·KPI·cohort)
- [ ] 6. 등록은 요청 시에만 semantic-register
```

### 1. Bound view

`cube.view`는 정본(상세) view 하나. view `columns[].name`이 dimension/fact `column` 및 expression 식별자의 기준이다. 사전 집계 요약 view가 있으면 `choice: [{ "view": "<summary_view>", "summary": true }]`와 `prefer: "summary"`를 둔다. 이미 등록됐으면 `GET /api/meta/view/{name}`으로 가져온다(물리 DW 조회용이 아님).

agent-surface 기본: **타깃당 cube 하나**, `name` = `target`.

### 2–4. Draft

[contract.md](contract.md) 해당 절. 형태:

- [examples/cubes/sales.json](examples/cubes/sales.json)
- [examples/metrics/revenue.json](examples/metrics/revenue.json) — **예시일 뿐**, 모든 큐브에 복제하지 말 것
- [examples/segments/paid.json](examples/segments/paid.json)

파생·고급 식 예:
- dimension/fact `expression`(스칼라만, D28): `date_trunc('year', order_date)`, `YEAR(order_date)`, `SUBSTR(order_date, 1, 4)`, `sale_amount * fx_rate`, 행 비율 `a / b`(`additive: false`), `IIF(cond, t, f)` 플래그. **집계를 dim/fact expression에 넣지 않는다.** 그룹 `SUM(a)/SUM(b)`는 metric.
- metric `expression` (집계·윈도우·서브쿼리 허용):
  - 기본 집계/비율: `SUM(sale_amount)`, `SUM(IIF(status_code = 'PAID', 1, 0)) * 100 / COUNT(*)`(권장; `DIVIDE(…)`는 호환 별칭)
  - 윈도우 함수: `RANK() OVER (PARTITION BY region_code ORDER BY SUM(sale_amount) DESC)`, `SUM(sale_amount) / SUM(SUM(sale_amount)) OVER ()`
  - 시맨틱/물리 서브쿼리: `SUM(sale_amount) / (SELECT SUM(sale_amount) FROM sales)`
- segment `filter.expression` (단순 비교 및 서브쿼리 허용):
  - 단순 조건: `status_code = 'PAID'`
  - 서브쿼리 코호트: `region_code IN (SELECT region_code FROM sales GROUP BY region_code HAVING SUM(sale_amount) > 1000)`

룩업 라벨 dim `type`은 기본적으로 `string`이면 충분하다.

### 5. Hand off

**cube 골격 직후 `semantic-from-qa`**로 description·이름 붙은 metric/segment를 채운다.  
실행 정확도 eval·그레인 분할은 **`semantic-from-eval`**.  
등록 순서: cube → metric | segment. 검증·CRUD는 **`semantic-register`**.

## Do not

- table/view를 여기서 만들지 않는다.
- 타깃 DW 물리 탐색에 MCP를 쓰지 않는다.
- `COUNT`/`AVG` 등 자명한 metric·일회성 segment를 휴리스틱으로 대량 생성하지 않는다.
- 외부 레포 경로를 정본으로 삼지 않는다 — 이 스킬 폴더만.
