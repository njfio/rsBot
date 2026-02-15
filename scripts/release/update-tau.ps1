[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$InstallDir = "",
    [string]$BinaryName = "tau-coding-agent",
    [switch]$Force,
    [switch]$DryRun,
    [switch]$NoVerify,
    [switch]$PrintTarget
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptPath = Join-Path $PSScriptRoot "install-tau.ps1"
$Params = @{
    Update = $true
}

if ($PSBoundParameters.ContainsKey("Version")) {
    $Params["Version"] = $Version
}
if ($PSBoundParameters.ContainsKey("InstallDir")) {
    $Params["InstallDir"] = $InstallDir
}
if ($PSBoundParameters.ContainsKey("BinaryName")) {
    $Params["BinaryName"] = $BinaryName
}
if ($Force) {
    $Params["Force"] = $true
}
if ($DryRun) {
    $Params["DryRun"] = $true
}
if ($NoVerify) {
    $Params["NoVerify"] = $true
}
if ($PrintTarget) {
    $Params["PrintTarget"] = $true
}

& $ScriptPath @Params
