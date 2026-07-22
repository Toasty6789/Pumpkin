@echo off
set "PATH=C:\Users\Toast\.cargo\bin;%PATH%"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
if errorlevel 1 exit /b 1
cargo %*
