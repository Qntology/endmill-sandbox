@echo off
chcp 65001 > nul
echo ========================================
echo   End Mill Sandbox - CUDA Dev Run
echo ========================================
echo.

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
set "NVCC_CCBIN=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64\cl.exe"
set "PYTHONIOENCODING=utf-8"
set "NVCC_PREPEND_FLAGS=-Xcompiler /Zc:preprocessor"

if not defined PROTOC (
    where protoc >nul 2>nul
    if errorlevel 1 (
        if not exist "%~dp0tools\protoc\bin\protoc.exe" (
            echo [SETUP] LanceDB 빌드용 protoc 내려받는 중...
            powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; New-Item -ItemType Directory -Force -Path '%~dp0tools' | Out-Null; Invoke-WebRequest -Uri 'https://github.com/protocolbuffers/protobuf/releases/download/v29.3/protoc-29.3-win64.zip' -OutFile '%~dp0tools\protoc.zip' -UseBasicParsing; Expand-Archive -Force '%~dp0tools\protoc.zip' '%~dp0tools\protoc'"
        )
        set "PROTOC=%~dp0tools\protoc\bin\protoc.exe"
    )
)

if not exist "node_modules" (
    echo [SETUP] node_modules 없음 - npm install 실행...
    call npm install
    if errorlevel 1 (
        echo [ERROR] npm install 실패!
        pause
        exit /b 1
    )
)

cd src-tauri

set "PATH=%CD%\dlls;%PATH%"

set "TAURI_CMD=npm"
for /f "tokens=2" %%v in ('cargo tauri --version 2^>nul') do set "TAURI_VER=%%v"
if defined TAURI_VER if "%TAURI_VER:~0,2%"=="1." set "TAURI_CMD=cargo"

echo [DEV] Starting Tauri application (Development Mode, CUDA)...

if "%TAURI_CMD%"=="cargo" (
    cargo tauri dev -- --features cuda
) else (
    call "%~dp0node_modules\.bin\tauri.cmd" dev -- --features cuda
)
pause