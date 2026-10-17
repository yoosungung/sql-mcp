#!/usr/bin/env bash
# BIRD Mini-Dev SQLite → target_db/<db_id>/<db_id>.sqlite
# + 질문-SQL JSON → target_db/bird_qa/mini_dev_sqlite.json
# 기본: 공식 Alibaba minidev.zip (HF LFS는 지역에 따라 403일 수 있음)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CACHE="${ROOT}/.cache"
QA_DIR="${ROOT}/bird_qa"
ZIP_URL="${BIRD_MINIDEV_ZIP_URL:-https://bird-bench.oss-cn-beijing.aliyuncs.com/minidev.zip}"
ZIP_PATH="${CACHE}/minidev.zip"
QA_MEMBER="minidev/MINIDEV/mini_dev_sqlite.json"
QA_DEST="${QA_DIR}/mini_dev_sqlite.json"

ALL_DBS=(
  california_schools
  card_games
  codebase_community
  debit_card_specializing
  european_football_2
  financial
  formula_1
  student_club
  superhero
  thrombosis_prediction
  toxicology
)

SMALL_DBS=(
  superhero
  student_club
  toxicology
  thrombosis_prediction
  california_schools
  formula_1
)

DBS=("${ALL_DBS[@]}")
if [[ "${1:-}" == "--small" ]]; then
  DBS=("${SMALL_DBS[@]}")
fi

mkdir -p "$CACHE"

need_download=0
for db in "${DBS[@]}"; do
  if [[ ! -s "${ROOT}/${db}/${db}.sqlite" ]]; then
    need_download=1
    break
  fi
done

if [[ "$need_download" -eq 1 ]]; then
  if [[ ! -s "$ZIP_PATH" ]]; then
    echo "download: $ZIP_URL"
    curl -fL --retry 3 --retry-delay 2 -A "sql-mcp-fetch/1.0" -o "${ZIP_PATH}.partial" "$ZIP_URL"
    mv "${ZIP_PATH}.partial" "$ZIP_PATH"
  else
    echo "reuse zip: $ZIP_PATH"
  fi
  echo "extract sqlite from zip…"
  for db in "${DBS[@]}"; do
    dest_dir="${ROOT}/${db}"
    dest="${dest_dir}/${db}.sqlite"
    mkdir -p "$dest_dir"
    if [[ -s "$dest" ]]; then
      echo "skip (exists): $db"
      continue
    fi
    member="minidev/MINIDEV/dev_databases/${db}/${db}.sqlite"
    unzip -p "$ZIP_PATH" "$member" >"${dest}.partial"
    mv "${dest}.partial" "$dest"
    echo "ok: $db ($(du -h "$dest" | awk '{print $1}'))"
  done
else
  echo "all requested DBs already present"
fi

# 질문-SQL (NL ↔ gold SQL). client/ 예제·평가용.
if [[ ! -s "$ZIP_PATH" ]]; then
  echo "download: $ZIP_URL"
  curl -fL --retry 3 --retry-delay 2 -A "sql-mcp-fetch/1.0" -o "${ZIP_PATH}.partial" "$ZIP_URL"
  mv "${ZIP_PATH}.partial" "$ZIP_PATH"
fi
mkdir -p "$QA_DIR"
if [[ -s "$QA_DEST" ]]; then
  echo "skip (exists): bird_qa/mini_dev_sqlite.json"
else
  echo "extract Q-SQL: $QA_MEMBER"
  unzip -p "$ZIP_PATH" "$QA_MEMBER" >"${QA_DEST}.partial"
  mv "${QA_DEST}.partial" "$QA_DEST"
  echo "ok: bird_qa/mini_dev_sqlite.json ($(du -h "$QA_DEST" | awk '{print $1}'))"
fi

echo "done → $ROOT"
ls -lh "${ROOT}"/*/*.sqlite 2>/dev/null | awk '{print $5, $9}'
ls -lh "$QA_DEST" 2>/dev/null | awk '{print $5, $9}'
