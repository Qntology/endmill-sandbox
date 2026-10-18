@echo off
rem ============================================================
rem  End Mill Sandbox - ROCm / HIP test run (AMD).
rem    test_rocm.bat              full sequential run (test_runner.bat): test_logs\<timestamp>_rocm\
rem    test_rocm.bat <cargo args> one cargo test with those arguments (old behaviour)
rem  HIP SDK version: set ROCM_VER (default in _env_rocm.bat). MSVC toolset: set VCVARS_VER.
rem  Options: NOPAUSE=1  QUICK=1  ONLY=04,11  LEAN_LINK=1  CARGO_BUILD_JOBS=2  MATRIX_PATTERNS=...  APP_WEAR_LOG_DIR=...
rem ============================================================
chcp 65001 > nul
setlocal EnableExtensions
call "%~dp0_env_rocm.bat"
if errorlevel 1 exit /b 1

set "BACKEND=rocm"
set "FEAT=--no-default-features --features rocm"
set "ENDMILL_DEVICE=rocm"

if "%~1"=="" goto :full
cd /d "%~dp0src-tauri"
set "PATH=%CD%\dlls;%PATH%"
cd /d "%~dp0"
echo [TEST] cargo test (ROCm / HIP %ROCM_VER%: %ROCM_PATH%) %*
cargo test %FEAT% %*
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%

:full
call "%~dp0test_runner.bat"
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%
