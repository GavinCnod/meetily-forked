@echo off

REM 清理 Next.js 开发构建缓存，避免 WebView 命中陈旧 chunk。
if exist .next (
  echo Cleaning Next.js cache...
  rd /s /q .next
)

REM 清理静态导出目录，避免调试时混入旧产物。
if exist out (
  echo Cleaning exported frontend...
  rd /s /q out
)

echo Cleaning npm dependencies...
rd /s /q node_modules
del /f /q package-lock.json

echo Installing npm dependencies...
pnpm install

echo Building the project...
pnpm run tauri dev
