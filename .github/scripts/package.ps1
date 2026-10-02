$ErrorActionPreference = 'Stop'
$version = (Get-Content Cargo.toml | Select-String '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$target = $env:RELEASE_TARGET
if (-not $target) { throw 'RELEASE_TARGET is required' }
$archive = "antigravity-worker-mcp-v$version-$target.zip"
New-Item -ItemType Directory -Force release, artifacts | Out-Null
Copy-Item "target/$target/release/antigravity-worker-mcp.exe" release/
Copy-Item LICENSE, THIRD_PARTY_NOTICES.md, README.md, CONTRIBUTING.md, SECURITY.md, CHANGELOG.md release/
Copy-Item -Recurse licenses, docs, examples release/
Compress-Archive -Path release/* -DestinationPath "artifacts/$archive" -Force
$hash = (Get-FileHash "artifacts/$archive" -Algorithm SHA256).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText("$PWD/artifacts/$archive.sha256", "$hash  $archive`n", [System.Text.UTF8Encoding]::new($false))
& ./release/antigravity-worker-mcp.exe --version
if ($LASTEXITCODE -ne 0) { throw 'Packaged executable failed its version check' }
