#!/usr/bin/env bash
# env-report.sh — 러너 환경을 주석과 step summary 로 남긴다 (logis-center 의 같은 스크립트를 이 저장소에 맞춘 것).
# 빌드 실패를 설명할 사실(컴파일러 · SDK · 입력)을 빌드 전에 적어 둔다. GitHub 는 단계당 알림 주석을 10개까지만 보여 주므로
# 네 묶음으로 올린다. 작업을 실패시키지 않는다. Git Bash (Windows) · bash (Linux · macOS) 공용.
set -u
. "$(dirname "$0")/lib.sh"
os="$(uname -s 2>/dev/null || echo unknown)"
case "$os" in MINGW*|MSYS*|CYGWIN*) win=1 ;; *) win=0 ;; esac

have() { command -v "$1" >/dev/null 2>&1; }
# line 이름 명령... : 명령 출력(stdout+stderr)의 앞 3줄, CR 제거
line() { local n="$1"; shift; local v; v="$("$@" 2>&1 | head -3 | tr -d '\r' | tr '\n' '|')" || true; printf '%s: %s\n' "$n" "${v:-<none>}"; }
present() { local n="$1" v="${2:-}"; if [ -n "$v" ]; then printf '%s: %s\n' "$n" "${v:0:160}"; else printf '%s: <unset>\n' "$n"; fi; }

emit() {
  echo "::notice title=env / $1::$(printf '%s\n' "$2" | escape)"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    { echo "#### env / $1"; echo '```'; printf '%s\n' "$2"; echo '```'; } >> "$GITHUB_STEP_SUMMARY"
  fi
}

# ── 1. 러너 ──
g1="$(
  echo "uname: $(uname -a 2>/dev/null | cut -c1-200)"
  if [ "$win" = 1 ]; then
    line "windows" powershell -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem | % { $_.Caption + " " + $_.Version + " " + $_.OSArchitecture })'
    line "memory-GB" powershell -NoProfile -Command '[math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB,1)'
    echo "cores: ${NUMBER_OF_PROCESSORS:-?}"
    line "disk-free" powershell -NoProfile -Command '(Get-PSDrive -Name (Get-Location).Drive.Name | % { [math]::Round($_.Free/1GB,1).ToString() + " GB free" })'
  elif [ "$os" = Darwin ]; then
    line "os" sw_vers
    line "cpu" sysctl -n machdep.cpu.brand_string
    echo "cores: $(sysctl -n hw.ncpu 2>/dev/null)"
    echo "memory-GB: $(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 ))"
    line "disk-free" bash -c "df -h . | tail -1"
    line "bash" bash --version
    line "metal-gpu" bash -c "system_profiler SPDisplaysDataType 2>/dev/null | grep -E 'Chipset Model|Metal' | head -2"
  else
    line "os" bash -c '. /etc/os-release 2>/dev/null; echo "$PRETTY_NAME"'
    echo "cores: $(nproc 2>/dev/null)"
    line "memory-MB" bash -c "free -m | awk '/Mem:/{print \$2\" total, \"\$7\" available\"}'"
    line "disk-free" bash -c "df -h . | tail -1"
  fi
  echo "image: ${ImageOS:-?} ${ImageVersion:-?}"
)"
emit "runner" "$g1"

# ── 2. 도구 ──
g2="$(
  line "rustc" rustc -vV
  line "cargo" cargo -V
  line "git" git --version
  line "protoc" protoc --version
  line "pwsh" pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'
  if [ "$win" = 1 ]; then
    vsw="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
    if [ -x "$vsw" ]; then
      line "vswhere" "$vsw" -all -products '*' -format value -property installationVersion -property installationPath
    else
      echo "vswhere: <not found>"
    fi
    line "cl (PATH)" where.exe cl
    line "link (PATH)" where.exe link
    line "autocrlf" git config --get core.autocrlf
    line "eol" git ls-files --eol test_runner.bat test_linux.sh .github/scripts/lib.sh
  else
    line "cc" cc --version
  fi
)"
emit "toolchain" "$g2"

# ── 3. GPU SDK ──
g3="$(
  if have nvcc; then line "nvcc" bash -c 'nvcc --version | tail -2'; else echo "nvcc: <not in PATH>"; fi
  present "CUDA_PATH" "${CUDA_PATH:-}"
  present "NVCC_CCBIN" "${NVCC_CCBIN:-}"
  present "CUDA_COMPUTE_CAP" "${CUDA_COMPUTE_CAP:-}"
  if have hipcc; then line "hipcc" hipcc --version; else echo "hipcc: <not in PATH>"; fi
  present "CANDLE_ROCM_SKIP_KERNEL_BUILD" "${CANDLE_ROCM_SKIP_KERNEL_BUILD:-}"
  present "VK_ICD_FILENAMES" "${VK_ICD_FILENAMES:-}"
  if [ "$win" = 1 ]; then
    line "vulkan-1.dll" where.exe vulkan-1.dll
  elif have vulkaninfo; then
    line "vulkaninfo" bash -c 'vulkaninfo --summary 2>&1 | grep -E "deviceName|apiVersion|driverName" | head -4'
  else
    echo "vulkaninfo: <not installed>"
  fi
)"
emit "gpu-sdk" "$g3"

# ── 4. 빌드 입력 ──
g4="$(
  echo "candle: $(grep -m1 'candle-rocm-vulkan' Cargo.lock | tr -d '\r')"
  echo "packages in Cargo.lock: $(grep -c '^\[\[package\]\]' Cargo.lock)"
  present "PROTOC" "${PROTOC:-}"
  present "RUSTFLAGS" "${RUSTFLAGS:-}"
  env | grep -E '^CARGO_(PROFILE|INCREMENTAL|BUILD)' | sort
)"
emit "build-inputs" "$g4"
exit 0
