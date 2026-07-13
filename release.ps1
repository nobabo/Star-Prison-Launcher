$ErrorActionPreference = "Stop"

Set-Location -LiteralPath $PSScriptRoot

node .\.github\workflows\sync-package-version.mjs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

node .\.github\workflows\release-nsis.mjs
exit $LASTEXITCODE
