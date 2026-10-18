#!/usr/bin/env bash
# 리눅스에서 endmill_sandbox 테스트를 백엔드별로 단계마다 실행합니다 — Windows 의 test_runner.bat 와 같은 단계·로그.
#
#   ./test_linux.sh [cpu|vulkan|rocm|cuda]              # 전체 단계 → test_logs/<시각>_<백엔드>/
#   ./test_linux.sh vulkan --test vulkan_backend        # 인자가 더 있으면 cargo test 한 번만 (예전 방식)
#   QUICK=1 ONLY=04,11 MATRIX_PATTERNS=rect_profile ./test_linux.sh cpu
#
# 단계: 00 환경 · 01 빌드 · 02~08 통합 시험 · 09 단위 시험 · 10 MPM · 11 공구 수명·N:N 시험 · 12 전 프리셋 감사 ·
#       13 전 프리셋 포켓 × 수명 정책 · 14 N:N 행렬(여러 경로) + 물리 점검 · 15 GPU↔CPU 대조 · 16 Vulkan 셰이더 대조
# 절삭 시뮬레이션은 백엔드와 관계없이 CPU 계산이라 02~14 결과는 모든 백엔드에서 같아야 합니다.
#
# 필요 패키지(Ubuntu 24.04 기준): protobuf-compiler (LanceDB), libvulkan1 mesa-vulkan-drivers (Vulkan)
# ROCm 은 /opt/rocm (또는 ROCM_PATH/HIP_PATH) 이 필요합니다.
set -uo pipefail
cd "$(dirname "$0")"

BACKEND="${1:-vulkan}"
[ $# -gt 0 ] && shift
case "$BACKEND" in
  cpu) FEATURE_ARGS=() ;;
  cuda|vulkan|rocm) FEATURE_ARGS=(--no-default-features --features "$BACKEND") ;;
  *) echo "unknown backend: $BACKEND (cpu|vulkan|rocm|cuda)"; exit 1 ;;
esac

if [ -z "${PROTOC:-}" ] && ! command -v protoc >/dev/null 2>&1; then
  echo "[SETUP] protoc 가 없습니다. 'sudo apt install protobuf-compiler' 또는 PROTOC=<경로> 를 지정하세요."; exit 1
fi

# GPU 가 없는 머신에서는 Mesa llvmpipe(lavapipe) 로 Vulkan 을 검증합니다.
# (포크는 기본적으로 CPU 타입 Vulkan 디바이스를 건너뜀. 실제 GPU 가 있으면 그것이 0번으로 선택됨)
if [ "$BACKEND" = vulkan ]; then export CANDLE_VULKAN_ALLOW_CPU="${CANDLE_VULKAN_ALLOW_CPU:-1}"; fi
export ENDMILL_DEVICE="$BACKEND"
export RUST_BACKTRACE=1

# 앱 데이터 디렉터리를 오염시키지 않도록 격리
XDG_DATA_HOME="$(mktemp -d)"; export XDG_DATA_HOME
trap 'rm -rf "$XDG_DATA_HOME"' EXIT

if [ $# -gt 0 ]; then
  cargo test "${FEATURE_ARGS[@]}" "$@"
  exit $?
fi

TS="$(date +%Y%m%d_%H%M%S)"
LOGDIR="$PWD/test_logs/${TS}_${BACKEND}"
mkdir -p "$LOGDIR/wear_logs" "$LOGDIR/matrix"
export ENDMILL_WEAR_LOG_DIR="$LOGDIR/wear_logs"
export ENDMILL_MATRIX_DIR="$LOGDIR/matrix"
MATRIX_PATTERNS="${MATRIX_PATTERNS:-rect_profile,pocket_zigzag,helical_pocket}"
SUMMARY="$LOGDIR/summary.log"
FAILED=0
{
  echo "========================================"
  echo " End Mill Sandbox test run $TS  backend=$BACKEND  features=${FEATURE_ARGS[*]:-}"
  echo " options: QUICK=${QUICK:-} ONLY=${ONLY:-} CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-} MATRIX_PATTERNS=$MATRIX_PATTERNS"
  echo "========================================"
} > "$SUMMARY"
echo "Logs: $LOGDIR"

{
  echo "--- date ---"; date
  echo "--- git ---"; git rev-parse HEAD; git status --short; git log -1 --format=%s
  echo "--- rustc / cargo ---"; rustc -Vv; cargo -V
  echo "--- backend $BACKEND ---"
  case "$BACKEND" in
    cuda) nvcc --version; nvidia-smi ;;
    rocm) "${ROCM_PATH:-/opt/rocm}/bin/hipcc" --version; command -v rocminfo >/dev/null && rocminfo | grep -m 3 -E 'Marketing Name|Name:' ;;
    vulkan) command -v vulkaninfo >/dev/null && vulkaninfo --summary ;;
  esac
  echo "--- cpu / memory / disk ---"
  (command -v lscpu >/dev/null && lscpu | grep -E 'Model name|^CPU\(s\)|Thread') || true
  free -g || true
  df -h . || true
  echo "--- env ---"
  echo "BACKEND=$BACKEND ENDMILL_DEVICE=$ENDMILL_DEVICE CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-}"
} > "$LOGDIR/00_env.log" 2>&1

# run <NN> <name> <long|-> <command...>
run() {
  local id="$1" name="$2" long="$3"; shift 3
  local step="${id}_${name}" log="$LOGDIR/${id}_${name}.log"
  if [ -n "${ONLY:-}" ] && [ "$id" != "01" ] && [[ ",${ONLY}," != *",${id},"* ]]; then
    echo "[$step] skipped - not in ONLY=$ONLY"; echo "$step: skipped - ONLY" >> "$SUMMARY"; return 0
  fi
  if [ -n "${QUICK:-}" ] && [ "$long" = long ]; then
    echo "[$step] skipped - QUICK"; echo "$step: skipped - QUICK" >> "$SUMMARY"; return 0
  fi
  echo "[$step] $*"
  local t0; t0=$(date +%s)
  { echo "[$(date +%T)] start $step  backend=$BACKEND"; echo "\$ $*"; echo; } > "$log"
  "$@" >> "$log" 2>&1
  local rc=$?
  local dur=$(( $(date +%s) - t0 ))
  { echo; echo "[$(date +%T)] end $step exit=$rc (${dur} s)"; } >> "$log"
  [ $rc -ne 0 ] && FAILED=$((FAILED + 1))
  echo "$step: exit=$rc  ${dur} s" >> "$SUMMARY"
  grep -E '^(test result:|error)' "$log" >> "$SUMMARY" || true
  grep -E 'FAILED|panicked at' "$log" >> "$SUMMARY" || true
  echo "  -> exit=$rc  ${dur} s  ($log)"
}

audit_all() {
  local rc=0
  for i in 0 1 2 3 4 5 6 7; do
    echo "===== preset $i rect_profile ====="
    cargo run "${FEATURE_ARGS[@]}" --example sim_audit -- "$i" rect_profile || rc=1
  done
  return $rc
}

CT=(cargo test "${FEATURE_ARGS[@]}")
CR=(cargo run "${FEATURE_ARGS[@]}")
NC=(--no-fail-fast -- --nocapture --test-threads=1)

run 01 build_tests - "${CT[@]}" --no-run
run 02 environment_tools - "${CT[@]}" --test environment_tools "${NC[@]}"
run 03 tool_state long "${CT[@]}" --test tool_state "${NC[@]}"
run 04 wear_compensation - "${CT[@]}" --test wear_compensation "${NC[@]}"
run 05 store_flow - "${CT[@]}" --test store_flow "${NC[@]}"
run 06 pipeline_flow - "${CT[@]}" --test pipeline_flow "${NC[@]}"
run 07 interaction_physics - "${CT[@]}" --test interaction_physics "${NC[@]}"
run 08 physcal_flow - "${CT[@]}" --test physcal_flow "${NC[@]}"
run 09 lib_unit - "${CT[@]}" --lib "${NC[@]}"
run 10 endmill_mpm - bash -c 'cd crates/endmill_mpm && cargo test --release --no-fail-fast -- --nocapture'
run 11 tool_life_matrix - "${CT[@]}" --test tool_life_matrix "${NC[@]}"
run 12 sim_audit_presets - audit_all
run 13 eol_probe long "${CR[@]}" --example eol_probe -- pocket_zigzag stop,change_tool,finish_part,run_to_failure
run 14 nn_matrix long "${CR[@]}" --example pair_matrix -- - "$MATRIX_PATTERNS" --check
if [ "$BACKEND" = cpu ]; then
  echo "15_gpu_parity: skipped - cpu backend" >> "$SUMMARY"
else
  run 15 gpu_parity - "${CT[@]}" --test gpu_backend "${NC[@]}"
fi
if [ "$BACKEND" = vulkan ]; then
  run 16 vulkan_backend - "${CT[@]}" --test vulkan_backend "${NC[@]}"
else
  echo "16_vulkan_backend: skipped - vulkan only" >> "$SUMMARY"
fi

{
  echo "========================================"
  if [ "$FAILED" -eq 0 ]; then echo "ALL STEPS PASSED"; else echo "STEPS FAILED: $FAILED"; fi
} >> "$SUMMARY"

# 요약: pwsh 가 있으면 Windows 와 같은 test_summary.ps1, 없으면 grep 으로 핵심만
TAG='^(test [^ ]+ \.\.\. )?\[(EOL|MATRIX|MATRIX-GRID|MATRIX-CHECK|AUDIT|UNKNOWN|GPU|COAT|TEMP|ENGAGE|GATES)'
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -File ./test_summary.ps1 -LogDir "$LOGDIR" -Backend "$BACKEND"
else
  : > "$LOGDIR/highlights.log"
  for f in "$LOGDIR"/[0-9][0-9]_*.log; do
    n=$(grep -cE "$TAG" "$f" || true)
    [ "${n:-0}" -gt 0 ] || continue
    { echo "===== $(basename "$f") ($n 줄) ====="; grep -E "$TAG" "$f" | sed -E 's/^test [^ ]+ \.\.\. //'; echo; } >> "$LOGDIR/highlights.log"
  done
  {
    echo
    echo "======== 시뮬레이션 교차 점검 요약 (highlights.log 에 원문) ========"
    echo "백엔드: $BACKEND"
    grep -h '^\[MATRIX-CHECK\] 경로 ' "$LOGDIR/highlights.log" | sed 's/^/  /' || true
    echo "N:N 물리 점검 결함: $(grep -c '^\[MATRIX-CHECK\]\[결함\]' "$LOGDIR/highlights.log" || true)"
    echo "GPU 대조 줄: $(grep -c '^\[GPU\]' "$LOGDIR/highlights.log" || true)"
    echo "[UNKNOWN] 항목: $(grep -oE '^\[UNKNOWN\] [^ ]+' "$LOGDIR/highlights.log" | sort -u | awk '{print $2}' | paste -sd, - || true)"
    echo "실패한 테스트:"; grep -h -E '^test .* \.\.\. FAILED' "$LOGDIR"/[0-9][0-9]_*.log | sed 's/^/  /' || true
  } >> "$SUMMARY"
  (cd "$LOGDIR" && find . -type f -printf '%10s  %P\n' | sort -k2) > "$LOGDIR/manifest.txt"
fi
cat "$SUMMARY"
echo "Logs saved in: $LOGDIR"
exit "$FAILED"
