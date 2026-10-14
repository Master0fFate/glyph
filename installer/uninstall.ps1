#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Glyph"),
    [switch]$Quiet
)

$ErrorActionPreference = "SilentlyContinue"
$target = Join-Path $InstallDir "glyph.exe"
Get-Process -Name glyph | Where-Object { $_.Path -eq $target } | Stop-Process -Force

$classes = "HKCU:\Software\Classes"
$progId = "Glyph.Markdown"
foreach ($ext in @(".md", ".markdown", ".mdown", ".mkd", ".mkdn", ".mdwn", ".mmd", ".mermaid", ".txt", ".text")) {
    $extKey = "$classes\$ext"
    if (Test-Path $extKey) {
        Remove-ItemProperty -Path "$extKey\OpenWithProgids" -Name $progId
        $current = (Get-ItemProperty -Path $extKey -Name "(default)")."(default)"
        $previous = (Get-ItemProperty -Path "HKCU:\Software\Glyph\PreviousDefaults" -Name $ext)."$ext"
        if ($current -eq $progId) {
            if ($previous) { Set-Item -Path $extKey -Value $previous } else { Remove-ItemProperty -Path $extKey -Name "(default)" }
        }
    }
}
Remove-Item -Path "$classes\$progId" -Recurse -Force
Remove-Item -Path "$classes\Applications\glyph.exe" -Recurse -Force
Remove-Item -Path "HKCU:\Software\Glyph" -Recurse -Force
Remove-ItemProperty -Path "HKCU:\Software\RegisteredApplications" -Name "Glyph"
Remove-Item -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Glyph" -Recurse -Force
Remove-Item -Path (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Glyph.lnk") -Force

$signature = '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int eventId, uint flags, IntPtr item1, IntPtr item2);'
$shell32 = Add-Type -MemberDefinition $signature -Name Notify -Namespace GlyphUninstall -PassThru
$shell32::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

if ((Resolve-Path $InstallDir).Path -like "*\Programs\Glyph") {
    Remove-Item -Path $target -Force
    Start-Process -FilePath "cmd.exe" -ArgumentList "/c timeout /t 1 >nul & rmdir /s /q `"$InstallDir`"" -WindowStyle Hidden
}
if (-not $Quiet) { Write-Host "Glyph was removed." }
