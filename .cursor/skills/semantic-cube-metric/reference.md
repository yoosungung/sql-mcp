# cube · metric · segment — pointers

| 주제 | 위치 |
|------|------|
| 식·필터·role·encryption | `ARCHITECTURE.md` §1 |
| 필드·aggregation | `ARCHITECTURE.md` §4 |
| JSON Schema | `semantic/schema/document.schema.json` |
| 컴파일·식별자(D18) | `server/spec.md`, `ARCHITECTURE.md` §1 |
| 예시 | `semantic/examples/cubes/`, `metrics/`, `segments/` |

## Quick checks

- [ ] `cube.view` 존재, dimension/fact `column` ∈ view columns
- [ ] `column`과 `expression` 동시 사용 없음
- [ ] metric `name` ∉ 같은 cube의 dimension/fact names
- [ ] `aggregation=group` ⇒ `group_by` ⊆ view columns
- [ ] segment `filter.expression` non-empty boolean
- [ ] 식에 서브쿼리·다중 문장·쓰기 CTE 없음
