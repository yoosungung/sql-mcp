---
name: semantic-from-qa
description: >-
  Refines sql-mcp semantic cube, metric, and segment JSON from natural-language
  questions, domain knowledge (definitions, formulas, glossaries), and optional
  reference SQL. Use when improving an existing semantic layer with Q&A pairs or
  business knowledge—not for authoring table/view from scratch, HTTP /api/meta
  registration, or execution-accuracy eval (use semantic-from-eval).
---

# Semantic refine · from Q&A / knowledge

**방법론 스킬.** 벤치마크·특정 데이터셋에 묶지 않는다. 입력이 어디서 왔든(내부 FAQ, 분석 티켓, 벤치 등) 같은 절차를 쓴다.

선행: 대상 `table`/`view`/(가능하면) `cube` 초안이 있어야 한다. 없으면 `semantic-table-view` → `semantic-cube-metric-segment` 먼저.

문서 형태: [../semantic-cube-metric-segment/contract.md](../semantic-cube-metric-segment/contract.md).  
마이닝 규칙 상세: [methodology.md](methodology.md).

**/api/meta 등록은 이 스킬 밖**(요청 시 `semantic-register`). 실행 정확도 eval·그레인 분할의 **반복 루프**는 `semantic-from-eval`.

## Inputs

| 입력 | 필수 | 역할 |
|------|------|------|
| 기존 시맨틱 JSON (`table`/`view`/`cube`…) | 예 | 개선 대상 |
| 자연어 질문(들) | 예(1건+) | 어떤 KPI·cohort·축이 쓰이는지 |
| 도메인 지식 | 권장 | 용어 정의, 계산식, 코드값 의미, grain |
| 참고 SQL | 선택 | 물리→시맨틱 **번역 힌트**만 (복붙 금지) |

질문·지식·SQL을 한 레코드로 묶어도 되고(`question` + `knowledge`/`evidence` + `sql`), 형식이 달라도 위 네 역할만 채우면 된다.

## Workflow

```
Progress:
- [ ] 1. 범위: outdir · target · 개선할 cube/view 확정
- [ ] 2. 입력 정규화 (질문 / 지식 / 참고 SQL)
- [ ] 3. 재사용 단위 추출 → 후보 목록 (methodology.md)
- [ ] 4. 기존 cube에 merge (dim/fact 보강 · metric · segment)
- [ ] 5. grain 불일치 시 view/cube 분리 제안만 (필요 시 table-view·cube 스킬로)
- [ ] 6. contract quick checks; 등록은 요청 시 semantic-register
```

### 1–2. Scope · normalize

`{outdir}/{name}.json`에 평탄 기록(다른 시맨틱 스킬과 동일). 지식에서 **공식·동의어·필터 조건**을 분리해 적는다. 참고 SQL이 있으면 쓰는 테이블·조인·WHERE·집계만 요약하고, 식 전체를 metric/segment에 넣지 않는다.

### 3–4. Mine · merge

[methodology.md](methodology.md): cohort → `segment`, 재사용 KPI → `metric`, 분석 축 → `dimension`(필요 시 `fact`/`expression`).  
**질문 1건 = metric/segment 1개 금지.** 빈도·지식에서 이름이 붙은 개념만 승격. 기존 멤버와 합치고, Auto 휴리스틱보다 지식·질문 신호를 우선한다.

### 5–6. Grain · hand off

한 cube/view로 커버 못 하는 grain이면 새 view/cube 초안을 **제안**하고, 작성은 `semantic-table-view` / `semantic-cube-metric-segment`에 넘긴다. 검증·CRUD는 **`semantic-register`**.

## Do not

- table/view를 이 스킬에서 새로 만들지 않는다(제안만).
- 참고 SQL·벤치 gold 전체 문장을 metric/segment/`expression`에 그대로 넣지 않는다.
- 다중 문장 · 쓰기 SQL을 시맨틱 식에 넣지 않는다.
- 실행 결과 비교·EX/Soft-F1 등 **eval 하네스**를 여기서 돌리거나 설계하지 않는다(`semantic-from-eval`).
- 특정 벤치 경로·스키마·DB 목록을 전제로 두지 않는다.
- 사용자 요청 전 `/api/meta` 등록 금지.
