#!/usr/bin/env bash
# Fail if project-plan ids leak into source, comments or test names.
#
# Plan ids (Story/Task/Epic numbers, plan-doc section refs, personal notes) describe the
# planning process, not the code. They belong in docs/ and commit messages only.
set -euo pipefail

cd "$(dirname "$0")/.."

pattern='\b(Story|Task|Epic) [0-9]|§ ?[0-9]|PROJECT_PLAN|IMPLEMENTATION_PLAN|osku_'
paths=(crates web/src web/test web/worklet)

if matches=$(grep -rnE "$pattern" "${paths[@]}"); then
  echo "Project-plan references found in source (keep plan ids in docs/ and commit messages):" >&2
  echo "$matches" >&2
  exit 1
fi
echo "check-plan-refs: no project-plan references in ${paths[*]}"
