<#
.SYNOPSIS
    Builds Stellar Online, starts a local galaxy server and opens the graphical client.

.EXAMPLE
    ./run.ps1                         # server + client "pilot" on 127.0.0.1:7777
    ./run.ps1 -Player bob -Clients 2  # server + 2 clients (bob, bob2)
    ./run.ps1 -ServerOnly             # server only, in this console
    ./run.ps1 -ClientOnly -Address 1.2.3.4:7777 -Player bob
    ./run.ps1 -Release                # optimized build
#>
param(
    [string]$Player = "pilot",
    [int]$Port = 7777,
    [string]$Save = "default.galaxy.json",
    [string]$Galaxy = "Milky Way",
    [string]$Address = "",
    [int]$Clients = 1,
    [switch]$ServerOnly,
    [switch]$ClientOnly,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$profile_ = if ($Release) { "release" } else { "debug" }
$buildArgs = @("build", "-p", "stellar-server", "-p", "stellar-client")
if ($Release) { $buildArgs += "--release" }
cargo @buildArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$bin = Join-Path $PSScriptRoot "target/$profile_"
$serverExe = Join-Path $bin "stellar-server.exe"
$clientExe = Join-Path $bin "stellar-client.exe"
if (-not $Address) { $Address = "127.0.0.1:$Port" }

if ($ServerOnly) {
    & $serverExe $Save $Port $Galaxy
    exit $LASTEXITCODE
}

$server = $null
if (-not $ClientOnly) {
    # Server in its own console window, so its log stays visible.
    $server = Start-Process $serverExe -ArgumentList "`"$Save`"", $Port, "`"$Galaxy`"" -PassThru
    # Wait until the port accepts connections.
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline) {
        try { (New-Object Net.Sockets.TcpClient("127.0.0.1", $Port)).Close(); break }
        catch { Start-Sleep -Milliseconds 200 }
    }
}

try {
    $procs = @()
    for ($i = 1; $i -le $Clients; $i++) {
        $name = if ($i -eq 1) { $Player } else { "$Player$i" }
        $procs += Start-Process $clientExe -ArgumentList "`"$name`"", $Address -PassThru
    }
    $procs | Wait-Process
}
finally {
    if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id }
}
