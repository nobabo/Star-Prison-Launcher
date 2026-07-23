$ErrorActionPreference = "Stop"

Set-Location -LiteralPath $PSScriptRoot

node .\.github\scripts\watch-release-archives.mjs
exit $LASTEXITCODE
