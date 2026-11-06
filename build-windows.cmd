@echo off
setlocal EnableExtensions DisableDelayedExpansion
pushd "%~dp0" || exit /b 1

rem Reuse an existing workspace toolchain and crate cache when available.
if not defined RUSTUP_HOME if exist ".build\rustup\settings.toml" set "RUSTUP_HOME=%CD%\.build\rustup"
if not defined CARGO_HOME if exist ".build\cargo\registry\" set "CARGO_HOME=%CD%\.build\cargo"

set "RIT_BUILD_STATUS=1"
where.exe cargo.exe >nul 2>&1
if errorlevel 1 (
    >&2 echo Install Rust and the MSVC toolchain with rustup before building.
    goto finish
)

set "RIT_TARGET="
for /f "tokens=2" %%T in ('rustc -vV ^| findstr.exe /b /c:"host:"') do set "RIT_TARGET=%%T"
if not "%RIT_TARGET%"=="x86_64-pc-windows-msvc" if not "%RIT_TARGET%"=="aarch64-pc-windows-msvc" (
    >&2 echo Use the 64-bit Windows MSVC Rust toolchain. Detected target: "%RIT_TARGET%"
    goto finish
)

cargo build --locked --release --package ritkraken --bin ritkraken --target "%RIT_TARGET%" --target-dir target\windows
set "RIT_BUILD_STATUS=%ERRORLEVEL%"
if not "%RIT_BUILD_STATUS%"=="0" goto finish

set "RIT_EXE=%~dp0target\windows\%RIT_TARGET%\release\ritkraken.exe"
if not exist "%RIT_EXE%" (
    >&2 echo Build finished without the expected executable "%RIT_EXE%".
    set "RIT_BUILD_STATUS=1"
    goto finish
)
echo Built "%RIT_EXE%"

:finish
popd
exit /b %RIT_BUILD_STATUS%
