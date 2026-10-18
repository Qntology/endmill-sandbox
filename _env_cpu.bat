@echo off
rem MSVC build environment for the CPU-only build (test_cpu.bat). Same Visual Studio probing as _env_vulkan.bat,
rem without the GPU runtime checks. PROTOC (LanceDB build) is handled by test_runner.bat.

set "VS_SCRIPT="
set "VS_ARGS="
set "VS2022=%ProgramFiles%\Microsoft Visual Studio\2022"
for %%E in (Community Professional Enterprise BuildTools) do call :probe_vs "%VS2022%\%%E"
if not defined VS_SCRIPT call :probe_vs "%ProgramFiles(x86)%\Microsoft Visual Studio\18\BuildTools"
if not defined VS_SCRIPT call :probe_vswhere
if not defined VS_SCRIPT (
    echo [MSVC] Visual Studio C++ Build Tools not found. Install the "Desktop development with C++" workload.
    exit /b 1
)
set "VCVARS_ARG="
if defined VCVARS_VER set "VCVARS_ARG=-vcvars_ver=%VCVARS_VER%"
call "%VS_SCRIPT%" %VS_ARGS% %VCVARS_ARG% > "%TEMP%\vcvars_out.txt" 2>&1
echo [MSVC] requested=%VCVARS_VER% active=%VCToolsVersion%
set "PYTHONIOENCODING=utf-8"
exit /b 0

:probe_vs
if defined VS_SCRIPT exit /b 0
if exist "%~1\VC\Auxiliary\Build\vcvars64.bat" (
    set "VS_SCRIPT=%~1\VC\Auxiliary\Build\vcvars64.bat"
    exit /b 0
)
if exist "%~1\Common7\Tools\VsDevCmd.bat" (
    set "VS_SCRIPT=%~1\Common7\Tools\VsDevCmd.bat"
    set "VS_ARGS=-arch=x64 -host_arch=x64"
)
exit /b 0

:probe_vswhere
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" exit /b 0
for /f "usebackq delims=" %%I in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do call :probe_vs "%%I"
exit /b 0
