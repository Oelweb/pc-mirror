# Aus einer erhöhten PowerShell starten. Nur private Netze / lokales Subnetz.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ReceiverPath, [switch]$Remove)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (!(New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Administratorrechte erforderlich: PowerShell als Administrator öffnen und diesen Befehl erneut ausführen.'
}
$resolvedReceiver = (Resolve-Path -LiteralPath $ReceiverPath).Path
if ([IO.Path]::GetFileName($resolvedReceiver) -ne 'uxplay.exe') { throw 'ReceiverPath muss auf uxplay.exe zeigen.' }
$rulePrefix = 'PC-Mirror-' + ([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($resolvedReceiver.ToLowerInvariant())) -replace '[^a-zA-Z0-9]','')
foreach ($suffix in @('TCP','UDP','mDNS')) {
    Get-NetFirewallRule -Name "$rulePrefix-$suffix" -ErrorAction SilentlyContinue | Remove-NetFirewallRule
}
if ($Remove) { return }
New-NetFirewallRule -Name "$rulePrefix-TCP" -DisplayName 'PC-Mirror AirPlay TCP' -Direction Inbound -Action Allow -Profile Private -Program $resolvedReceiver -Protocol TCP -LocalPort '48000-48002' -RemoteAddress LocalSubnet | Out-Null
New-NetFirewallRule -Name "$rulePrefix-UDP" -DisplayName 'PC-Mirror AirPlay UDP' -Direction Inbound -Action Allow -Profile Private -Program $resolvedReceiver -Protocol UDP -LocalPort '48000-48002' -RemoteAddress LocalSubnet | Out-Null
New-NetFirewallRule -Name "$rulePrefix-mDNS" -DisplayName 'PC-Mirror mDNS' -Direction Inbound -Action Allow -Profile Private -Program $resolvedReceiver -Protocol UDP -LocalPort 5353 -RemoteAddress LocalSubnet | Out-Null
Write-Output 'Firewall eingerichtet: TCP/UDP 48000–48002 und UDP 5353, nur Privat / lokales Subnetz.'
