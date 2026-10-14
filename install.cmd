@echo off
setlocal
cd /d "%~dp0"
if exist "dist\glyph.exe" goto :install
where cargo >nul 2>nul
if errorlevel 1 (
    echo dist\glyph.exe is missing and cargo was not found. Install Rust from https://rustup.rs and run this again.
    exit /b 1
)
echo Building Glyph in release mode...
cargo build --release || exit /b 1
if not exist dist mkdir dist
copy /y "target\release\glyph.exe" "dist\glyph.exe" >nul
:install
powershell -NoProfile -ExecutionPolicy Bypass -File "installer\install.ps1" -Exe "%~dp0dist\glyph.exe"
