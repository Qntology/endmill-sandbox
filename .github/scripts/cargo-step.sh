#!/usr/bin/env bash
# cargo-step.sh <이름> <명령...> — test_linux.sh / test_cpu.bat 를 거치지 않는 빌드 · 타입 검사 단계.
# 출력은 test_logs/<이름>.log 에 남기고 (산출물로 올라간다), 실패하면 오류 줄을 주석으로 올린다.
set -u
. "$(dirname "$0")/lib.sh"
name="$1"; shift
mkdir -p test_logs
log="test_logs/$name.log"
echo "+ $*"
"$@" 2>&1 | tee "$log"
rc=${PIPESTATUS[0]}
[ "$rc" -eq 0 ] || annot "$name" "$log"
exit "$rc"
