@echo off
cd /d "%~dp0"
if exist "dist\omp-sync-client-windows-x64\omp-sync-client.exe" (
    start "" "dist\omp-sync-client-windows-x64\omp-sync-client.exe"
) else if exist "target\x86_64-pc-windows-gnullvm\release\omp-sync-client.exe" (
    start "" "target\x86_64-pc-windows-gnullvm\release\omp-sync-client.exe"
) else if exist "target\release\omp-sync-client.exe" (
    start "" "target\release\omp-sync-client.exe"
) else (
    cargo run --release
)
