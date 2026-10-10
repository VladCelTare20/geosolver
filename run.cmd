@echo off
REM Build and launch the GeoSolver web app (double-click friendly).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" %*
