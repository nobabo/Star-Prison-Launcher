@echo off
setlocal

for /f "delims=" %%I in ('rustc --print sysroot') do set "RUST_SYSROOT=%%I"
set "RUST_LLD=%RUST_SYSROOT%\lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"

if not exist "%RUST_LLD%" (
  echo rust-lld.exe not found at "%RUST_LLD%" 1>&2
  exit /b 1
)

if /i "%~1"=="-flavor" if /i "%~2"=="link" (
  shift
  shift
)

"%RUST_LLD%" %*
exit /b %ERRORLEVEL%

