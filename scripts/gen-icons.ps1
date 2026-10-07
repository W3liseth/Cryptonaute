# Génère les icônes de l'application (PNG + ICO multi-tailles) sans outil externe.
# Usage : pwsh scripts/gen-icons.ps1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$outDir = Join-Path $PSScriptRoot '..\crates\app\icons'
New-Item -ItemType Directory -Force $outDir | Out-Null

function New-IconBitmap([int]$size, [int[]]$from = @(124, 92, 255), [int[]]$to = @(34, 211, 238)) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $s = $size / 512.0
    # Carré arrondi avec dégradé (violet -> cyan par défaut)
    $r = 112 * $s; $m = 16 * $s; $w = $size - 2 * $m
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($m, $m, 2 * $r, 2 * $r, 180, 90)
    $path.AddArc($m + $w - 2 * $r, $m, 2 * $r, 2 * $r, 270, 90)
    $path.AddArc($m + $w - 2 * $r, $m + $w - 2 * $r, 2 * $r, 2 * $r, 0, 90)
    $path.AddArc($m, $m + $w - 2 * $r, 2 * $r, 2 * $r, 90, 90)
    $path.CloseFigure()
    $rect = New-Object System.Drawing.RectangleF 0, 0, $size, $size
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, `
        ([System.Drawing.Color]::FromArgb(255, $from[0], $from[1], $from[2])), `
        ([System.Drawing.Color]::FromArgb(255, $to[0], $to[1], $to[2])), 45.0
    $g.FillPath($brush, $path)

    # Bouclier blanc
    $shield = New-Object System.Drawing.Drawing2D.GraphicsPath
    $pts = @(
        @(256, 104), @(372, 146), @(372, 252), @(256, 412), @(140, 252), @(140, 146)
    ) | ForEach-Object { New-Object System.Drawing.PointF ($_[0] * $s), ($_[1] * $s) }
    $shield.AddBezier($pts[0], (New-Object System.Drawing.PointF (300 * $s), (128 * $s)), (New-Object System.Drawing.PointF (340 * $s), (140 * $s)), $pts[1])
    $shield.AddLine($pts[1], $pts[2])
    $shield.AddBezier($pts[2], (New-Object System.Drawing.PointF (372 * $s), (330 * $s)), (New-Object System.Drawing.PointF (310 * $s), (385 * $s)), $pts[3])
    $shield.AddBezier($pts[3], (New-Object System.Drawing.PointF (202 * $s), (385 * $s)), (New-Object System.Drawing.PointF (140 * $s), (330 * $s)), $pts[4])
    $shield.AddLine($pts[4], $pts[5])
    $shield.AddBezier($pts[5], (New-Object System.Drawing.PointF (172 * $s), (140 * $s)), (New-Object System.Drawing.PointF (212 * $s), (128 * $s)), $pts[0])
    $shield.CloseFigure()
    $g.FillPath([System.Drawing.Brushes]::White, $shield)

    # Éclair dégradé au centre du bouclier
    $bolt = New-Object System.Drawing.Drawing2D.GraphicsPath
    $boltPts = @(@(272, 160), @(196, 276), @(250, 276), @(236, 360), @(316, 236), @(262, 236), @(272, 160)) |
        ForEach-Object { New-Object System.Drawing.PointF ($_[0] * $s), ($_[1] * $s) }
    $bolt.AddPolygon([System.Drawing.PointF[]]$boltPts)
    $g.FillPath($brush, $bolt)

    $g.Dispose()
    return $bmp
}

$png = New-IconBitmap 512
$png.Save((Join-Path $outDir 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)

# Icônes de la zone de notification : gris (déconnecté) et vert (connecté)
$trayOff = New-IconBitmap 64 @(100, 106, 140) @(60, 64, 92)
$trayOff.Save((Join-Path $outDir 'tray-off.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$trayOn = New-IconBitmap 64 @(16, 185, 129) @(45, 212, 191)
$trayOn.Save((Join-Path $outDir 'tray-on.png'), [System.Drawing.Imaging.ImageFormat]::Png)

# ICO contenant des images PNG (format accepté depuis Windows Vista)
$sizes = 16, 24, 32, 48, 64, 128, 256
$images = foreach ($sz in $sizes) {
    $b = New-IconBitmap $sz
    $ms = New-Object System.IO.MemoryStream
    $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $b.Dispose()
    , $ms.ToArray()
}
$out = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $out
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $bw.Write([byte]$dim); $bw.Write([byte]$dim); $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([UInt16]1); $bw.Write([UInt16]32)
    $bw.Write([UInt32]$images[$i].Length); $bw.Write([UInt32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $bw.Write($img) }
$bw.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $outDir 'icon.ico'), $out.ToArray())
Write-Host "Icônes générées dans $outDir"
