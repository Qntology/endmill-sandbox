@echo off
rem ============================================================
rem  End Mill Sandbox - CUDA sequential test runner
rem  Every step writes its own log:  test_logs\<timestamp>\NN_name.log
rem  Failures do not stop the run. At the end:
rem    summary.log     exit code and duration of every step, test results, cross-check counts
rem    highlights.log  every simulation cross-check line ([EOL] [MATRIX] [AUDIT] [UNKNOWN] ...) in one file
rem    wear_logs\      CSV/JSON job logs written by this run (also mirrored to the app's wear_logs folder)
rem    matrix\         tool x material N:N matrix (CSV/JSON)
rem    manifest.txt    every file this run produced
rem
rem  Usage: test_cuda.bat
rem    set NOPAUSE=1    skip the final pause
rem    set QUICK=1      skip the long steps (03 tool_state, 13 eol_probe)
rem    set ONLY=04,11   run only these step numbers (step 01 build always runs)
rem    set LEAN_LINK=1  link this crate's test binaries without PDB debug info: much less link
rem                     memory and disk (2+ GB per test binary) on small machines. Dependencies are
rem                     not rebuilt. Panic messages keep file:line, backtraces lose symbol names.
rem    set CARGO_BUILD_JOBS=6  more parallel rustc/link jobs if the machine has the memory
rem ============================================================
chcp 65001 > nul
setlocal EnableExtensions

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
set "NVCC_CCBIN=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64\cl.exe"
set "PYTHONIOENCODING=utf-8"
set "NVCC_PREPEND_FLAGS=-Xcompiler /Zc:preprocessor"
set "RUST_BACKTRACE=1"
set "ENDMILL_DEVICE=cuda"
rem Limit parallel rustc/link jobs. With every test binary linked at once on a 7 GB machine, step 01 failed with
rem "os error 1455" (paging file too small) -> "invalid metadata files / only metadata stub found for std";
rem a single link.exe of a test binary peaks near 2.6 GB (LNK1102 "out of memory" when three overlap).
rem Override with: set CARGO_BUILD_JOBS=6 before running if this machine has more memory.
if not defined CARGO_BUILD_JOBS set "CARGO_BUILD_JOBS=2"
set "CARGO_EXTRA="
if defined LEAN_LINK set "CARGO_EXTRA=--config profile.dev.package.endmill_sandbox.debug=0 --config profile.dev.package.endmill_sandbox.strip=true"

if not defined PROTOC (
    where protoc >nul 2>nul
    if errorlevel 1 (
        if not exist "%~dp0tools\protoc\bin\protoc.exe" (
            echo [SETUP] protoc not found - downloading for LanceDB build...
            powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; New-Item -ItemType Directory -Force -Path '%~dp0tools' | Out-Null; Invoke-WebRequest -Uri 'https://github.com/protocolbuffers/protobuf/releases/download/v29.3/protoc-29.3-win64.zip' -OutFile '%~dp0tools\protoc.zip' -UseBasicParsing; Expand-Archive -Force '%~dp0tools\protoc.zip' '%~dp0tools\protoc'"
        )
        set "PROTOC=%~dp0tools\protoc\bin\protoc.exe"
    )
)

cd /d "%~dp0"
set "PATH=%CD%\src-tauri\dlls;%PATH%"

for /f %%i in ('powershell -NoProfile -Command "Get-Date -Format yyyyMMdd_HHmmss"') do set "TS=%%i"
set "LOGDIR=%CD%\test_logs\%TS%"
mkdir "%LOGDIR%"
mkdir "%LOGDIR%\wear_logs"
mkdir "%LOGDIR%\matrix"
set "SUMMARY=%LOGDIR%\summary.log"
set "FAILED=0"
rem wear_compensation saves its CSV/JSON job logs into this run's folder (mirrored to the app folder at the end);
rem tool_life_matrix saves the N:N matrix next to them
if not defined APP_WEAR_LOG_DIR set "APP_WEAR_LOG_DIR=%APPDATA%\com.qntology.endmill-sandbox\wear_logs"
set "ENDMILL_WEAR_LOG_DIR=%LOGDIR%\wear_logs"
set "ENDMILL_MATRIX_DIR=%LOGDIR%\matrix"

>"%SUMMARY%" echo ========================================
>>"%SUMMARY%" echo  End Mill Sandbox CUDA test run %TS%
>>"%SUMMARY%" echo  options: QUICK=%QUICK% ONLY=%ONLY% LEAN_LINK=%LEAN_LINK% CARGO_BUILD_JOBS=%CARGO_BUILD_JOBS%
>>"%SUMMARY%" echo ========================================
echo Logs: %LOGDIR%
echo.

rem ---- 00: environment snapshot ----
set "ENVLOG=%LOGDIR%\00_env.log"
echo [00] environment snapshot
(
  echo --- date ---
  echo %DATE% %TIME%
  echo --- git ---
  git rev-parse HEAD
  git status --short
  git log -1 --format=%%s
  echo --- rustc / cargo ---
  rustc -Vv
  cargo -V
  echo --- nvcc ---
  nvcc --version
  echo --- nvidia-smi ---
  nvidia-smi
  echo --- memory / disk ---
  powershell -NoProfile -Command "$o = Get-CimInstance Win32_OperatingSystem; 'RAM total {0:N1} GB, free {1:N1} GB' -f ($o.TotalVisibleMemorySize/1MB), ($o.FreePhysicalMemory/1MB); 'Disk C: free {0:N1} GB' -f ((Get-PSDrive C).Free/1GB)"
  echo --- env ---
  echo ENDMILL_DEVICE=%ENDMILL_DEVICE%
  echo PROTOC=%PROTOC%
  echo CARGO_BUILD_JOBS=%CARGO_BUILD_JOBS%
  echo CARGO_EXTRA=%CARGO_EXTRA%
  echo ENDMILL_WEAR_LOG_DIR=%ENDMILL_WEAR_LOG_DIR%
  echo ENDMILL_MATRIX_DIR=%ENDMILL_MATRIX_DIR%
) > "%ENVLOG%" 2>&1

rem ---- 01: build all test binaries and examples first (separates compile errors from test failures) ----
call :run 01 build_tests "cargo test --features cuda %CARGO_EXTRA% --no-run"

rem ---- 02..08: integration tests, in dependency order (preset/material list first) ----
call :run 02 environment_tools   "cargo test --features cuda %CARGO_EXTRA% --test environment_tools --no-fail-fast -- --nocapture --test-threads=1"
call :run 03 tool_state          "cargo test --features cuda %CARGO_EXTRA% --test tool_state --no-fail-fast -- --nocapture --test-threads=1" long
call :run 04 wear_compensation   "cargo test --features cuda %CARGO_EXTRA% --test wear_compensation --no-fail-fast -- --nocapture --test-threads=1"
call :run 05 store_flow          "cargo test --features cuda %CARGO_EXTRA% --test store_flow --no-fail-fast -- --nocapture --test-threads=1"
call :run 06 pipeline_flow       "cargo test --features cuda %CARGO_EXTRA% --test pipeline_flow --no-fail-fast -- --nocapture --test-threads=1"
call :run 07 interaction_physics "cargo test --features cuda %CARGO_EXTRA% --test interaction_physics --no-fail-fast -- --nocapture --test-threads=1"
call :run 08 physcal_flow        "cargo test --features cuda %CARGO_EXTRA% --test physcal_flow --no-fail-fast -- --nocapture --test-threads=1"

rem ---- 09: library unit tests ----
call :run 09 lib_unit            "cargo test --features cuda %CARGO_EXTRA% --lib --no-fail-fast -- --nocapture --test-threads=1"

rem ---- 10: MPM solver crate (release) ----
pushd crates\endmill_mpm
call :run 10 endmill_mpm         "cargo test --release --no-fail-fast -- --nocapture"
popd

rem ---- 11: tool end of life (stop / change / finish part / failure) and the tool x material N:N matrix ----
call :run 11 tool_life_matrix    "cargo test --features cuda %CARGO_EXTRA% --test tool_life_matrix --no-fail-fast -- --nocapture --test-threads=1"

rem ---- 12: full pipeline audit of every built-in preset (rect_profile): [AUDIT] [EOL] [UNKNOWN] lines ----
call :run 12 sim_audit_presets   "call :audit_all"

rem ---- 13: end-of-life behaviour of every preset on the pocket path under all four tool-life policies ----
call :run 13 eol_probe           "cargo run --features cuda %CARGO_EXTRA% --example eol_probe -- pocket_zigzag stop,change_tool,finish_part,run_to_failure" long

rem (tests\vulkan_backend.rs is compiled only with --features vulkan, so it is skipped here)

rem ---- mirror this run's wear logs into the app folder (the app lists them there) ----
if exist "%LOGDIR%\wear_logs\*" (
    if not exist "%APP_WEAR_LOG_DIR%" mkdir "%APP_WEAR_LOG_DIR%"
    copy /y "%LOGDIR%\wear_logs\*.*" "%APP_WEAR_LOG_DIR%\" >nul
    >>"%SUMMARY%" echo wear logs: %LOGDIR%\wear_logs  ^(mirrored to %APP_WEAR_LOG_DIR%^)
)

echo.
>>"%SUMMARY%" echo ========================================
if "%FAILED%"=="0" (
    >>"%SUMMARY%" echo ALL STEPS PASSED
) else (
    >>"%SUMMARY%" echo STEPS FAILED: %FAILED%
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0test_cuda_summary.ps1" -LogDir "%LOGDIR%"
echo.
type "%SUMMARY%"
echo.
echo Logs saved in: %LOGDIR%
echo Cross-check lines: %LOGDIR%\highlights.log
if not defined NOPAUSE pause
endlocal & exit /b %FAILED%

rem ------------------------------------------------------------
rem  :run <NN> <name> "<command>" [long]
rem ------------------------------------------------------------
:run
set "ID=%~1"
set "STEP=%~1_%~2"
set "CMDLINE=%~3"
set "IS_LONG=%~4"
set "STEPLOG=%LOGDIR%\%STEP%.log"
if not defined ONLY goto :run_quick
if "%ID%"=="01" goto :run_quick
echo ,%ONLY%, | findstr /C:",%ID%," >nul
if errorlevel 1 (
    echo [%STEP%] skipped - not in ONLY=%ONLY%
    >>"%SUMMARY%" echo %STEP%: skipped - ONLY
    exit /b 0
)
:run_quick
if not defined QUICK goto :run_go
if /i not "%IS_LONG%"=="long" goto :run_go
echo [%STEP%] skipped - QUICK
>>"%SUMMARY%" echo %STEP%: skipped - QUICK
exit /b 0
:run_go
echo [%STEP%] %CMDLINE%
for /f %%i in ('powershell -NoProfile -Command "[DateTimeOffset]::Now.ToUnixTimeSeconds()"') do set "T0=%%i"
>"%STEPLOG%" echo [%TIME%] start %STEP%
>>"%STEPLOG%" echo $ %CMDLINE%
>>"%STEPLOG%" echo.
%CMDLINE% >> "%STEPLOG%" 2>&1
set "RC=%ERRORLEVEL%"
for /f %%i in ('powershell -NoProfile -Command "[DateTimeOffset]::Now.ToUnixTimeSeconds()"') do set "T1=%%i"
set /a DUR=T1-T0
>>"%STEPLOG%" echo.
>>"%STEPLOG%" echo [%TIME%] end %STEP% exit=%RC% (%DUR% s)
if not "%RC%"=="0" set /a FAILED+=1
>>"%SUMMARY%" echo %STEP%: exit=%RC%  %DUR% s
findstr /B /C:"test result:" /C:"error" "%STEPLOG%" >> "%SUMMARY%" 2>nul
findstr /C:"FAILED" /C:"panicked at" "%STEPLOG%" >> "%SUMMARY%" 2>nul
echo   -^> exit=%RC%  %DUR% s  (%STEPLOG%)
exit /b 0

rem ------------------------------------------------------------
rem  :audit_all - sim_audit example for every built-in preset (0..7) on rect_profile
rem ------------------------------------------------------------
:audit_all
set "AUDIT_RC=0"
for /L %%i in (0,1,7) do (
    echo ===== preset %%i rect_profile =====
    cargo run --features cuda %CARGO_EXTRA% --example sim_audit -- %%i rect_profile
    if errorlevel 1 set "AUDIT_RC=1"
)
exit /b %AUDIT_RC%
