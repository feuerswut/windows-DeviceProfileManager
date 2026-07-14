@echo off
cd /d "%~dp0kaiser-app"
if not exist node_modules npm install
npm run tauri -- dev
