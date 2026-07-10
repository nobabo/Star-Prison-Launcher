$ErrorActionPreference = "Stop"

Set-Location -LiteralPath $PSScriptRoot

node .\dist\release-nsis.mjs
exit $LASTEXITCODE
