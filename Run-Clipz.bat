@echo off
title Clipz Desktop Launcher
cd /d "%~dp0"
set PATH=C:\Users\ACER\.cargo\bin;%PATH%
echo =========================================
echo   Starting Clipz Desktop Notch Hub...
echo =========================================
npm run tauri -- dev
pause
