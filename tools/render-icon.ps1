# Regenerate the original Fleab bitmap/icon family on Windows using GDI+.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$assetRoot = Join-Path $PSScriptRoot '..\assets'
$bitmap = [Drawing.Bitmap]::new(256,256)
$g = [Drawing.Graphics]::FromImage($bitmap)
$g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
$dark = [Drawing.ColorTranslator]::FromHtml('#302633')
$pen = [Drawing.Pen]::new($dark,9)
$pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
function Line($points) { for($i=0;$i -lt $points.Count-2;$i+=2) { $g.DrawLine($pen,[single]$points[$i],[single]$points[$i+1],[single]$points[$i+2],[single]$points[$i+3]) } }
function Ellipse($color,$x,$y,$w,$h) { $brush=[Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($color)); $g.FillEllipse($brush,$x,$y,$w,$h); $g.DrawEllipse($pen,$x,$y,$w,$h); $brush.Dispose() }
function Box($color,$x,$y,$w,$h) { $brush=[Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($color)); $g.FillRectangle($brush,$x,$y,$w,$h); $g.DrawRectangle($pen,$x,$y,$w,$h); $brush.Dispose() }
Line @(96,153,53,173,40,221); Line @(112,174,83,208,105,228); Line @(164,169,189,210,218,224); Line @(164,143,211,153,230,187)
Ellipse '#ae6649' 78 92 110 118
Line @(104,72,91,42); Line @(145,68,162,37)
Ellipse '#c8835c' 80 56 96 84
$hat = [Drawing.Drawing2D.GraphicsPath]::new(); $hat.AddArc(76,27,102,90,180,180); $hat.AddLine(178,72,76,72)
$brush=[Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#f4c443')); $g.FillPath($brush,$hat); $g.DrawPath($pen,$hat); Line @(69,77,185,77); Line @(127,29,127,59)
$eye=[Drawing.SolidBrush]::new($dark); $g.FillEllipse($eye,107,85,12,18); $g.FillEllipse($eye,139,85,12,18); $g.DrawArc($pen,117,104,29,18,0,170)
Box '#80653f' 85 179 89 20; Box '#f4c443' 118 181 21 21
Line @(176,132,203,120); Line @(84,132,62,122); Box '#dca368' 191 107 39 38; Line @(211,109,211,143)
$bitmap.Save((Join-Path $assetRoot 'fleab.png'),[Drawing.Imaging.ImageFormat]::Png)
$png=[IO.File]::ReadAllBytes((Join-Path $assetRoot 'fleab.png'))
$ico=[IO.MemoryStream]::new(); $writer=[IO.BinaryWriter]::new($ico)
$writer.Write([uint16]0);$writer.Write([uint16]1);$writer.Write([uint16]1)
$writer.Write([byte]0);$writer.Write([byte]0);$writer.Write([byte]0);$writer.Write([byte]0)
$writer.Write([uint16]1);$writer.Write([uint16]32);$writer.Write([uint32]$png.Length);$writer.Write([uint32]22);$writer.Write($png)
[IO.File]::WriteAllBytes((Join-Path $assetRoot 'fleab.ico'),$ico.ToArray())
function BigEndian($n) { $bytes=[BitConverter]::GetBytes([uint32]$n); [Array]::Reverse($bytes); return $bytes }
$icns=[IO.MemoryStream]::new();$icns.Write([Text.Encoding]::ASCII.GetBytes('icns'));$icns.Write([byte[]](BigEndian ($png.Length+16)),0,4);$icns.Write([Text.Encoding]::ASCII.GetBytes('ic08'));$icns.Write([byte[]](BigEndian ($png.Length+8)),0,4);$icns.Write($png)
[IO.File]::WriteAllBytes((Join-Path $assetRoot 'fleab.icns'),$icns.ToArray())
$writer.Dispose();$ico.Dispose();$icns.Dispose();$hat.Dispose();$brush.Dispose();$eye.Dispose();$pen.Dispose();$g.Dispose();$bitmap.Dispose()
