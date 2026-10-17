@echo off
rem ============================================================
rem  End Mill Sandbox - CUDA sequential test runner
rem  Every step writes its own log:  test_logs\<timestamp>\NN_name.log
rem  Failures do not stop the run; summary.log collects the results.
rem  Usage: test_cuda.bat            (set NOPAUSE=1 to skip the final pause)
rem ============================================================
chcp 65001 > nul
setlocal

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
rem wear_compensation test saves its CSV/JSON job logs here (same folder the app uses)
set "ENDMILL_WEAR_LOG_DIR=%APPDATA%\com.qntology.endmill-sandbox\wear_logs"

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
set "SUMMARY=%LOGDIR%\summary.log"
set "FAILED=0"

echo ======================================== > "%SUMMARY%"
echo  End Mill Sandbox CUDA test run %TS%     >> "%SUMMARY%"
echo ======================================== >> "%SUMMARY%"
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
  echo --- rustc / cargo ---
  rustc -Vv
  cargo -V
  echo --- nvcc ---
  nvcc --version
  echo --- nvidia-smi ---
  nvidia-smi
  echo --- env ---
  echo ENDMILL_DEVICE=%ENDMILL_DEVICE%
  echo PROTOC=%PROTOC%
) > "%ENVLOG%" 2>&1

rem ---- 01: build all test binaries first (separates compile errors from test failures) ----
call :run 01_build_tests "cargo test --features cuda --no-run"

rem ---- 02..08: integration tests, in dependency order (preset/material list first) ----
call :run 02_environment_tools   "cargo test --features cuda --test environment_tools --no-fail-fast -- --nocapture --test-threads=1"
call :run 03_tool_state          "cargo test --features cuda --test tool_state --no-fail-fast -- --nocapture --test-threads=1"
call :run 04_wear_compensation   "cargo test --features cuda --test wear_compensation --no-fail-fast -- --nocapture --test-threads=1"
call :run 05_store_flow          "cargo test --features cuda --test store_flow --no-fail-fast -- --nocapture --test-threads=1"
call :run 06_pipeline_flow       "cargo test --features cuda --test pipeline_flow --no-fail-fast -- --nocapture --test-threads=1"
call :run 07_interaction_physics "cargo test --features cuda --test interaction_physics --no-fail-fast -- --nocapture --test-threads=1"
call :run 08_physcal_flow        "cargo test --features cuda --test physcal_flow --no-fail-fast -- --nocapture --test-threads=1"

rem ---- 09: library unit tests ----
call :run 09_lib_unit            "cargo test --features cuda --lib --no-fail-fast -- --nocapture --test-threads=1"

rem ---- 10: MPM solver crate (release) ----
pushd crates\endmill_mpm
call :run 10_endmill_mpm         "cargo test --release --no-fail-fast -- --nocapture"
popd

rem (tests\vulkan_backend.rs is compiled only with --features vulkan, so it is skipped here)

echo.
echo ======================================== >> "%SUMMARY%"
if "%FAILED%"=="0" (
    echo ALL STEPS PASSED >> "%SUMMARY%"
) else (
    echo STEPS FAILED: %FAILED%  >> "%SUMMARY%"
)
echo.
type "%SUMMARY%"
echo.
echo Logs saved in: %LOGDIR%
if not defined NOPAUSE pause
endlocal & exit /b %FAILED%

rem ------------------------------------------------------------
rem  :run <log-name> "<command>"
rem ------------------------------------------------------------
:run
set "STEP=%~1"
set "CMDLINE=%~2"
set "STEPLOG=%LOGDIR%\%STEP%.log"
echo [%STEP%] %CMDLINE%
echo [%TIME%] start %STEP% > "%STEPLOG%"
echo $ %CMDLINE%           >> "%STEPLOG%"
echo.                      >> "%STEPLOG%"
%CMDLINE% >> "%STEPLOG%" 2>&1
set "RC=%ERRORLEVEL%"
echo.                      >> "%STEPLOG%"
echo [%TIME%] end %STEP% exit=%RC% >> "%STEPLOG%"
if not "%RC%"=="0" set /a FAILED+=1
echo %STEP%: exit=%RC%     >> "%SUMMARY%"
findstr /B /C:"test result:" /C:"error" "%STEPLOG%" >> "%SUMMARY%" 2>nul
findstr /C:"FAILED" /C:"panicked at" "%STEPLOG%" >> "%SUMMARY%" 2>nul
echo   -^> exit=%RC%  (%STEPLOG%)
exit /b 0
