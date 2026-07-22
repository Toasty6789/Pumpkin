@echo off
REM Pumpkin build helper — sets up VS 2022 Build Tools env + Cargo
set "PATH=C:\Users\Toast\.cargo\bin;%PATH%"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" > nul 2>&1
if errorlevel 1 (
  echo VS env setup failed
  exit /b 1
)
cargo %*
