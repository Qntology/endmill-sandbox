#!/usr/bin/env bash
# publish-results.sh <백엔드> — test_linux.sh / test_cpu.bat 가 남긴 test_logs/<시각>_<백엔드>/ 를 주석과 step summary 로 올린다.
# 실패한 단계마다 오류 주석 하나, 단계별 종료 코드 · 시간과 교차 점검 요약은 알림 주석으로 남긴다.
# 합격 판정은 테스트 단계의 종료 코드가 한다. 여기서는 결과가 없거나 장치 시험이 실제로 돌지 않은 경우만 실패로 돌린다.
set -u
. "$(dirname "$0")/lib.sh"
backend="${1:?backend}"

dir="$(ls -dt test_logs/*_"$backend" 2>/dev/null | head -1)"
if [ -z "$dir" ] || [ ! -f "$dir/summary.log" ]; then
  echo "::error title=results::NOT RUN — test_logs/*_$backend/summary.log 이 없다 (테스트 단계가 시작되지 않았거나 중간에 끊겼다)"
  exit 1
fi
summary="$dir/summary.log"

# 단계 줄 형식: "NN_name: exit=N  D s" / "NN_name: skipped - 이유"
tr -d '\r' < "$summary" | sed -nE 's/^([0-9][0-9]_[a-z0-9_]+): exit=([1-9][0-9]*).*/\1 \2/p' | while read -r step rc; do
  annot "$step (exit $rc)" "$dir/$step.log"
done

echo "::notice title=steps::$(tr -d '\r' < "$summary" | grep -E '^[0-9][0-9]_[a-z0-9_]+: ' | escape)"
cross="$(tr -d '\r' < "$summary" | sed -n '/^======== /,$p')"
[ -z "$cross" ] || echo "::notice title=cross-check::$(printf '%s\n' "$cross" | escape)"

# 컴파일 경고는 개수만으로는 알 수 없으니 로그를 받지 않아도 보이도록 종류별로 올린다
warnings="$(cat "$dir"/[0-9][0-9]_*.log | tr -d '\r' | grep -E '^warning: ' | grep -vE 'generated [0-9]+ warning' | cut -c1-200 | sort | uniq -c | sort -rn | head -12)"
[ -z "$warnings" ] || echo "::notice title=warnings::$(printf '%s\n' "$warnings" | escape)"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  { echo "### $backend"; echo '```'; tr -d '\r' < "$summary" | head -c 900000; echo; echo '```'; } >> "$GITHUB_STEP_SUMMARY"
fi

# 장치 시험은 실제로 돌았다는 증거 줄이 있어야 한다 (건너뛴 시험은 통과가 아니다)
if [ "$backend" != cpu ]; then
  device="$(tr -d '\r' < "$dir/highlights.log" 2>/dev/null | grep -m1 -E "^\[GPU\] $backend · 장치")"
  if [ -z "$device" ]; then
    echo "::error title=device::NOT RUN — [GPU] 장치 줄이 없다. 장치 시험이 돌지 않았다"
    exit 1
  fi
  echo "::notice title=device::$device"
fi
