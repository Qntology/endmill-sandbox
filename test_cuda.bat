@echo off
rem ============================================================
rem  End Mill Sandbox - CUDA test run (NVIDIA). The steps, logs and options live in test_runner.bat:
rem    test_logs\<timestamp>_cuda\  NN_name.log, summary.log, highlights.log, wear_logs\, matrix\, manifest.txt
rem  Options: NOPAUSE=1  QUICK=1  ONLY=04,11  LEAN_LINK=1  CARGO_BUILD_JOBS=2  MATRIX_PATTERNS=...  APP_WEAR_LOG_DIR=...
rem  Other backends: test_rocm.bat (AMD HIP), test_vulkan.bat (Vulkan), test_cpu.bat (CPU only)
rem ============================================================
chcp 65001 > nul
setlocal EnableExtensions

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
set "NVCC_CCBIN=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64\cl.exe"
set "PYTHONIOENCODING=utf-8"
set "NVCC_PREPEND_FLAGS=-Xcompiler /Zc:preprocessor"

set "BACKEND=cuda"
set "FEAT=--features cuda"
set "ENDMILL_DEVICE=cuda"
call "%~dp0test_runner.bat"
set "RC=%ERRORLEVEL%"
endlocal & exit /b %RC%
