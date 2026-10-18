@echo off
rem ============================================================
rem  End Mill Sandbox - CPU-only test run (no GPU feature; ML runs on the CPU).
rem    test_cpu.bat              full sequential run (test_runner.bat): test_logs\<timestamp>_cpu\
rem    test_cpu.bat <cargo args> one cargo test with those arguments
rem  The cutting simulation runs on the CPU for every backend, so steps 02-14 here are the reference that the
rem  cuda / rocm / vulkan runs must match. Steps 15-16 (GPU parity) are skipped.
rem  Options: NOPAUSE=1  QUICK=1  ONLY=04,11  LEAN_LINK=1  CARGO_BUILD_JOBS=2  MATRIX_PATTERNS=...  APP_WEAR_LOG_DIR=...
rem ============================================================
chcp 65001 > nul
setlocal EnableExtensions
call "%~dp0_env_cpu.bat"
if errorlevel 1 exit /b 1

set "BACKEND=cpu"
set "FEAT="
set "ENDMILL_DEVICE=cpu"

if "%~1"=="" goto :full
cd /d "%~dp0src-tauri"
set "PATH=%CD%\dlls;%PATH%"
cd /d "%~dp0"
echo [TEST] cargo test (CPU) %*
cargo test %*
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%

:full
call "%~dp0test_runner.bat"
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%
