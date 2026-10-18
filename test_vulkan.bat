@echo off
rem ============================================================
rem  End Mill Sandbox - Vulkan test run.
rem    test_vulkan.bat              full sequential run (test_runner.bat): test_logs\<timestamp>_vulkan\
rem    test_vulkan.bat <cargo args> one cargo test with those arguments (old behaviour), e.g.
rem                                 test_vulkan.bat --test vulkan_backend
rem  Options: NOPAUSE=1  QUICK=1  ONLY=04,11  LEAN_LINK=1  CARGO_BUILD_JOBS=2  MATRIX_PATTERNS=...  APP_WEAR_LOG_DIR=...
rem  Without a GPU set CANDLE_VULKAN_ALLOW_CPU=1 to test on a CPU Vulkan device (llvmpipe / lavapipe / WARP).
rem ============================================================
chcp 65001 > nul
setlocal EnableExtensions
call "%~dp0_env_vulkan.bat"
if errorlevel 1 exit /b 1

set "BACKEND=vulkan"
set "FEAT=--no-default-features --features vulkan"
set "ENDMILL_DEVICE=vulkan"

if "%~1"=="" goto :full
cd /d "%~dp0src-tauri"
set "PATH=%CD%\dlls;%PATH%"
cd /d "%~dp0"
echo [TEST] cargo test (Vulkan) %*
cargo test %FEAT% %*
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%

:full
call "%~dp0test_runner.bat"
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%
