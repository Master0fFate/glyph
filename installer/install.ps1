#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Exe,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Glyph"),
    [switch]$NoDefault
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

if (-not $Exe) {
    $candidates = @(
        (Join-Path $PSScriptRoot "glyph.exe"),
        (Join-Path $root "dist\glyph.exe"),
        (Join-Path $root "target\release\glyph.exe")
    )
    $Exe = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Exe -or -not (Test-Path $Exe)) {
    throw "glyph.exe was not found. Run 'cargo build --release' first or pass -Exe <path>."
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$target = Join-Path $InstallDir "glyph.exe"
Get-Process -Name glyph -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $target } | Stop-Process -Force -ErrorAction SilentlyContinue
Copy-Item -Path $Exe -Destination $target -Force
Copy-Item -Path (Join-Path $PSScriptRoot "uninstall.ps1") -Destination (Join-Path $InstallDir "uninstall.ps1") -Force

$classes = "HKCU:\Software\Classes"
$progId = "Glyph.Markdown"
$command = "`"$target`" `"%1`""

function Set-RegValue([string]$Path, [string]$Name, [string]$Value) {
    if (-not (Test-Path $Path)) { New-Item -Path $Path -Force | Out-Null }
    if ($Name) { New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType String -Force | Out-Null }
    else { Set-Item -Path $Path -Value $Value }
}

Set-RegValue "$classes\$progId" "" "Markdown Document"
Set-RegValue "$classes\$progId" "FriendlyTypeName" "Markdown Document"
Set-RegValue "$classes\$progId\DefaultIcon" "" "`"$target`",0"
Set-RegValue "$classes\$progId\shell" "" "open"
Set-RegValue "$classes\$progId\shell\open" "" "Open with Glyph"
Set-RegValue "$classes\$progId\shell\open" "Icon" "`"$target`",0"
Set-RegValue "$classes\$progId\shell\open\command" "" $command

Set-RegValue "$classes\Applications\glyph.exe" "FriendlyAppName" "Glyph"
Set-RegValue "$classes\Applications\glyph.exe\DefaultIcon" "" "`"$target`",0"
Set-RegValue "$classes\Applications\glyph.exe\shell\open\command" "" $command

$defaultExtensions = @(".md", ".markdown", ".mdown", ".mkd", ".mkdn", ".mdwn", ".mmd", ".mermaid")
$optionalExtensions = @(".txt", ".text")

foreach ($ext in $defaultExtensions + $optionalExtensions) {
    $extKey = "$classes\$ext"
    if (-not (Test-Path $extKey)) { New-Item -Path $extKey -Force | Out-Null }
    Set-RegValue "$extKey\OpenWithProgids" $progId ""
    Set-RegValue "$classes\Applications\glyph.exe\SupportedTypes" $ext ""
}
$previous = "HKCU:\Software\Glyph\PreviousDefaults"
if (-not $NoDefault) {
    foreach ($ext in $defaultExtensions) {
        $extKey = "$classes\$ext"
        $existing = (Get-ItemProperty -Path $extKey -Name "(default)" -ErrorAction SilentlyContinue)."(default)"
        if ($existing -and $existing -ne $progId -and -not (Get-ItemProperty -Path $previous -Name $ext -ErrorAction SilentlyContinue)) {
            Set-RegValue $previous $ext $existing
        }
        Set-Item -Path $extKey -Value $progId
        Set-RegValue "$extKey" "PerceivedType" "text"
        Set-RegValue "$extKey" "Content Type" "text/markdown"
        $userChoice = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext\UserChoice"
        if (Test-Path $userChoice) {
            $chosen = (Get-ItemProperty -Path $userChoice -Name ProgId -ErrorAction SilentlyContinue).ProgId
            if ($chosen -ne $progId) {
                try {
                    $parent = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext", $true)
                    $parent.DeleteSubKeyTree("UserChoice")
                    $parent.Close()
                }
                catch { Write-Warning "Windows kept an earlier 'Open with' choice for $ext ($chosen). If $ext files still open elsewhere, right-click one, choose Open with > Glyph, and tick Always." }
            }
        }
    }
}

$capabilities = "HKCU:\Software\Glyph\Capabilities"
Set-RegValue $capabilities "ApplicationName" "Glyph"
Set-RegValue $capabilities "ApplicationDescription" "A native Markdown editor for notes, code and diagrams."
Set-RegValue $capabilities "ApplicationIcon" "`"$target`",0"
foreach ($ext in $defaultExtensions + $optionalExtensions) {
    Set-RegValue "$capabilities\FileAssociations" $ext $progId
}
Set-RegValue "HKCU:\Software\RegisteredApplications" "Glyph" "Software\Glyph\Capabilities"

$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$shortcut = Join-Path $startMenu "Glyph.lnk"
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
$link.TargetPath = $target
$link.WorkingDirectory = $InstallDir
$link.IconLocation = "$target,0"
$link.Description = "Glyph Markdown Editor"
$link.Save()

$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Glyph"
$uninstallCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstallDir\uninstall.ps1`""
Set-RegValue $uninstallKey "DisplayName" "Glyph"
Set-RegValue $uninstallKey "DisplayVersion" (Get-Item $target).VersionInfo.ProductVersion
Set-RegValue $uninstallKey "Publisher" "Glyph contributors"
Set-RegValue $uninstallKey "DisplayIcon" "$target,0"
Set-RegValue $uninstallKey "InstallLocation" $InstallDir
Set-RegValue $uninstallKey "UninstallString" $uninstallCommand
Set-RegValue $uninstallKey "QuietUninstallString" "$uninstallCommand -Quiet"
New-ItemProperty -Path $uninstallKey -Name "NoModify" -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "NoRepair" -Value 1 -PropertyType DWord -Force | Out-Null

$signature = '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int eventId, uint flags, IntPtr item1, IntPtr item2);'
$shell32 = Add-Type -MemberDefinition $signature -Name Notify -Namespace Glyph -PassThru
$shell32::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host "Glyph installed to $target"
Write-Host "Markdown files (.md, .markdown, .mdown, .mkd, .mmd, .mermaid) now open in Glyph; .txt is available under 'Open with'."
Write-Host "Uninstall from Settings > Apps, or run: $InstallDir\uninstall.ps1"
