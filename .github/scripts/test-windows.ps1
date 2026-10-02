$ErrorActionPreference = 'Stop'
if (-not $env:RELEASE_TARGET -or -not $env:RUST_VERSION) { throw 'Target and toolchain are required' }

# Cargo places its children in a job that forbids breakaway. Compile first,
# then run account-free test binaries outside Cargo and the CI runner's job.
function Quote-PS([string] $Value) { return "'" + $Value.Replace("'", "''") + "'" }
$cargo = (Get-Command cargo).Source
$workspace = $PWD.Path
$artifacts = & $cargo ('+' + $env:RUST_VERSION) test --locked --target $env:RELEASE_TARGET --all-features --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Windows test compilation failed' }
$executables = @($artifacts | ForEach-Object {
    $artifact = $_ | ConvertFrom-Json
    if ($artifact.reason -eq 'compiler-artifact' -and $artifact.profile.test -and $artifact.executable) { $artifact.executable }
})
if ($executables.Count -lt 3) { throw 'Compiled Windows test executables are missing' }
$executableJson = ConvertTo-Json -InputObject $executables -Compress
$output = Join-Path ([System.IO.Path]::GetTempPath()) ('agymcp-ci-' + [guid]::NewGuid())
New-Item -ItemType Directory $output | Out-Null
$log = Join-Path $output 'test.log'
$result = Join-Path $output 'exit-code.txt'
$cargoDirectory = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustDirectory = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$script = @"
`$ErrorActionPreference = 'Stop'
`$PSNativeCommandUseErrorActionPreference = `$false
`$env:USERPROFILE = $(Quote-PS $env:USERPROFILE)
`$env:CARGO_HOME = $(Quote-PS $cargoDirectory)
`$env:RUSTUP_HOME = $(Quote-PS $rustDirectory)
`$code = 1
try {
    Set-Location $(Quote-PS $workspace)
    `$executables = $(Quote-PS $executableJson) | ConvertFrom-Json
    `$code = 0
    foreach (`$executable in `$executables) {
        & `$executable *>> $(Quote-PS $log)
        if (`$LASTEXITCODE -ne 0) { `$code = `$LASTEXITCODE; break }
    }
} catch {
    `$_ | Out-File -Append $(Quote-PS $log)
}
[System.IO.File]::WriteAllText($(Quote-PS $result), [string]`$code)
exit `$code
"@
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
$powershell = (Get-Command pwsh).Source
$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ CreateFlags = [uint32]0x01000000; ShowWindow = [uint16]0 }
$created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
    CommandLine = '"' + $powershell + '" -NoProfile -NonInteractive -EncodedCommand ' + $encoded
    CurrentDirectory = $workspace
    ProcessStartupInformation = $startup
}
if ($created.ReturnValue -ne 0) {
    Remove-Item -Recurse -Force $output
    throw "Independent CI process creation failed: $($created.ReturnValue)"
}
Write-Host "Independent Windows test process: $($created.ProcessId)"
try {
    $deadline = [DateTime]::UtcNow.AddMinutes(20)
    while (-not (Test-Path $result)) {
        if (-not (Get-Process -Id $created.ProcessId -ErrorAction SilentlyContinue)) { throw 'Windows CI process exited without a result' }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Windows CI test process did not finish' }
        Start-Sleep -Milliseconds 250
    }
    Get-Content $log
    $code = [int](Get-Content $result -Raw)
} finally {
    if (-not (Test-Path $result)) { Stop-Process -Id $created.ProcessId -ErrorAction SilentlyContinue }
    Remove-Item -Recurse -Force $output
}
if ($code -ne 0) { throw "Windows tests failed with exit code $code" }
& $cargo ('+' + $env:RUST_VERSION) test --locked --target $env:RELEASE_TARGET --all-features --doc
if ($LASTEXITCODE -ne 0) { throw 'Windows documentation tests failed' }
