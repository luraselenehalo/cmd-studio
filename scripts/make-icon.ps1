<#
.SYNOPSIS
  Draws CMD Studio's placeholder icon (a terracotta square with ">_") as
  app\src-tauri\icons\icon.ico (256x256) and, as the source for the other sizes, a 1024x1024
  PNG at %TEMP%\icon-1024.png, with System.Drawing. Tauri's build embeds the .ico as the exe's
  icon (tauri-build reads bundle.icon); `npx tauri icon %TEMP%\icon-1024.png` makes every other
  size from the PNG once the npm packages are installed. The PNG stays in %TEMP%, outside the
  repository. The founder may replace both with a designed icon at any time; nothing else
  refers to their pixels.
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent
$iconDir = Join-Path $root 'app\src-tauri\icons'
New-Item -ItemType Directory -Force $iconDir | Out-Null

function Draw([int] $size) {
  $bmp = New-Object System.Drawing.Bitmap $size, $size
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias
  $g.Clear([System.Drawing.ColorTranslator]::FromHtml('#d97757'))
  $font = New-Object System.Drawing.Font('Consolas', [float]($size * 0.47), [System.Drawing.FontStyle]::Bold)
  $brush = New-Object System.Drawing.SolidBrush([System.Drawing.ColorTranslator]::FromHtml('#faf9f5'))
  $g.DrawString('>_', $font, $brush, [float]($size * 0.04), [float]($size * 0.2))
  $g.Dispose()
  return $bmp
}

$pngPath = Join-Path $env:TEMP 'icon-1024.png'
$png = Draw 1024
$png.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
$png.Dispose()

# The .ico holds one 256x256 image stored as PNG (as `tauri icon` writes it; Windows Vista and
# later, rc.exe and the shell read it). Not `Icon.FromHandle($bmp.GetHicon()).Save()`: on
# Windows 11 that writes a 4-bit, 16-colour image, and the terracotta comes out grey.
$small = Draw 256
$data = New-Object System.IO.MemoryStream
$small.Save($data, [System.Drawing.Imaging.ImageFormat]::Png)
$small.Dispose()
$image = $data.ToArray()
$data.Dispose()
$head = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $head
$w.Write([uint16]0)              # reserved
$w.Write([uint16]1)              # type: icon
$w.Write([uint16]1)              # one image
$w.Write([byte]0)                # width 0 means 256
$w.Write([byte]0)                # height 0 means 256
$w.Write([byte]0)                # no palette
$w.Write([byte]0)                # reserved
$w.Write([uint16]1)              # colour planes
$w.Write([uint16]32)             # bits per pixel
$w.Write([uint32]$image.Length)  # size of the image
$w.Write([uint32]22)             # offset of the image: 6 + 16
$w.Flush()
$bytes = [byte[]] ($head.ToArray() + $image)
$w.Dispose()
[System.IO.File]::WriteAllBytes((Join-Path $iconDir 'icon.ico'), $bytes)
Write-Host "wrote $pngPath and app\src-tauri\icons\icon.ico"
