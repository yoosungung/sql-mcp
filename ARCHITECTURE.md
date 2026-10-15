# ARCHITECTURE

## 1. 계약

이 서버는 두 에이전트가 쓴다.

코딩 에이전트(Claude, Cursor, Antigravity)는 git에서 JSON을 소스로 작성하고 MCP 메타로 등록한다. 등록 문서는 `semantic/schema/`의 JSON Schema와 `sql_mcp_semantic.validate`를 통과해야 한다.

DW 에이전트는 메타 등록 도구를 쓰지 않는다. 자연어로 컬럼을 찾고, 테이블 목록을 보고, 테이블과 샘플을 본 뒤 SELECT를 작성한다. 이 에이전트에게 테이블은 cube이고 컬럼은 dimension, fact, metric이다.

SQL 실행은 한 개의 SELECT만 받는다. AST로 SELECT가 아닌 문을 거절한 뒤, 등록된 문서를 따라 물리 SQL을 만들어 실행한다.

테이블 필터의 식은 물리 컬럼 이름을 쓴다. cube, metric, segment의 식은 그 cube의 dimension, fact 이름을 쓴다. 필터 식은 코딩 에이전트가 작성한 신뢰된 SQL 조각이다.

컴파일할 때 아래 필터를 모두 AND로 붙인다.

1. 선택된 view가 닿는 각 table의 `scope_filter`
2. 호출자 role과 같은 table `permission_filters`
3. 각 table의 `default_filter`
4. 질의에 쓰인 dimension, fact의 `default_filter`
5. 질의에 쓰인 metric의 `filter`
6. 질의에 쓰인 segment의 `filter`

dimension과 fact의 `role`이 비어 있지 않으면 호출자는 그 중 하나의 role을 가져야 한다. `encryption`이 true인 컬럼은 원문으로 반환하지 않는다.

view를 join, union, minus로 만들 때 이름과 데이터와 코드가 같은 컬럼은 하나의 view 컬럼로 묶는다. `mappings`가 둘 이상이면 `same_name`, `same_data`, `same_code`가 모두 true이고, 가리키는 컬럼 이름이 같아야 한다.

`combine`이 `select`인 view는 view만 묶는다. 사용자 SQL이 참조한 컬럼을 모두 가진 source 중에서 `summary`가 true인 view를 우선 선택한다. 요약 view로 풀 수 없으면 나머지 source를 쓴다. 어느 source도 컬럼을 덮지 못하면 질의를 거절한다.

## 2. 등록 문서

디렉터리 이름이 문서 종류다. 파일명(확장자 제외)은 `name`과 같다.

```text
semantic/examples/
  tables/{name}.json
  views/{name}.json
  cubes/{name}.json
  metrics/{name}.json
  segments/{name}.json
```

참조는 `table → view → cube → metric|segment` 방향이다. cube는 view 하나에 연결된다. metric과 segment는 cube 하나에 연결된다. metric 이름은 같은 cube의 dimension, fact 이름과 겹칠 수 없다.

필드 타입의 정본은 `semantic/schema/*.schema.json`이다. `$id`는 `https://sql-mcp.local/semantic/` 아래의 문서 식별자이며 다운로드 주소가 아니다.

| 문서 | 필수 필드 |
| --- | --- |
| table | `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter` |
| view | `kind`, `name`, `combine`, `sources`, `columns`. join은 `joins`, select는 `prefer` |
| cube | `kind`, `name`, `view`, `description`, `dimensions`, `facts` |
| metric | `kind`, `name`, `cube`, `expression`, `aggregation`, `filter` |
| segment | `kind`, `name`, `cube`, `filter` |

table `columns`는 `name`, `type`, `nullable`이다. `pk`는 그 컬럼 이름이다. `fk.columns`는 이 테이블 컬럼이고 `fk.references`는 다른 논리 테이블과 그 컬럼이다.

view `combine`은 `from`, `join`, `union`, `minus`, `select`다. `from`은 테이블 하나를 cube에 올릴 때 쓴다. `join`, `union`, `minus`의 source는 테이블이고, `select`의 source는 view다. union과 minus는 첫 source가 기준이고 각 view 컬럼이 모든 source를 가리킨다. join의 `type`은 `inner`, `left`, `right`, `full`이다.

dimension은 `code`, `date`, `time`을 가진다. fact는 `amount`, `fx`를 가진다. 둘 다 `default_filter`, `encryption`, `role`을 가진다. `column`은 연결된 view 컬럼이다.

metric `expression`은 집계 전의 스칼라 식이고 `aggregation`은 `sum`, `avg`, `min`, `max`, `count`, `count_distinct` 중 하나다. segment `filter`는 비울 수 없다.

예시는 `semantic/examples/`에 있다.
