$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ArgonInteraction {
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,IntPtr id);
 [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a,uint b,bool attach);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
 [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int ht,bool r);
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h,int a,out RECT r,int s);
 public struct RECT { public int Left,Top,Right,Bottom; }
}
'@
$work = $env:GITHUB_WORKSPACE
$exe = (Get-Item "$work/dist/argon.exe").FullName
$config = Join-Path $env:APPDATA 'argon/settings.json'
$test = Join-Path $env:RUNNER_TEMP 'argon-interaction.html'
Set-Content $test '<html><head><title>JS pending</title></head><body><h1 id="s">JS pending</h1><script>document.title="Argon JS OK"; document.getElementById("s").textContent="Argon JS OK";</script></body></html>'
$url = 'file:///' + ($test -replace '\\','/')
function Start-Argon {
 $script:proc = Start-Process $exe -ArgumentList $url -WorkingDirectory (Split-Path $exe) -PassThru
 for ($i=0;$i -lt 60;$i++) {
  Start-Sleep 1; $script:proc.Refresh()
  if ($script:proc.HasExited) { throw 'Argon exited during interaction test' }
  if ($script:proc.MainWindowTitle -like 'Argon JS OK*') { break }
 }
 if ($script:proc.MainWindowTitle -notlike 'Argon JS OK*') { throw 'Actual JS title mutation not observed' }
 $wa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
 [ArgonInteraction]::MoveWindow($script:proc.MainWindowHandle,$wa.X+10,$wa.Y+10,$wa.Width-20,$wa.Height-20,$true) | Out-Null
 [ArgonInteraction]::SetForegroundWindow($script:proc.MainWindowHandle) | Out-Null
 $script:root = [System.Windows.Automation.AutomationElement]::FromHandle($script:proc.MainWindowHandle)
 Start-Sleep 2
}
function Find-Named($name) {
 $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name)
 for ($i=0;$i -lt 15;$i++) {
  $e=$script:root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$cond)
  if ($e) { return $e }; Start-Sleep -Milliseconds 400
 }
 $all = $script:root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 $all | ForEach-Object { "$($_.Current.Name) | $($_.Current.ControlType.ProgrammaticName)" } | Set-Content "$work/interaction-tree.txt"
 Capture 'argon-settings-failure.png'
 Get-Content "$work/interaction-tree.txt" | Write-Host
 throw "Named accessibility control missing: $name"
}
function Click-Element($e) {
 $select = $null
 if ($e.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$select)) {
  $select.Select(); Start-Sleep -Milliseconds 900; return
 }
 $toggle = $null
 if ($e.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$toggle)) {
  $toggle.Toggle(); Start-Sleep -Milliseconds 900; return
 }
 $invoke = $null
 if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invoke)) {
  $invoke.Invoke()
  Start-Sleep -Milliseconds 900
  return
 }
 [ArgonInteraction]::SetForegroundWindow($script:proc.MainWindowHandle) | Out-Null
 Start-Sleep -Milliseconds 500
 $r=$e.Current.BoundingRectangle
 if ($r.IsEmpty -or $r.Width -lt 1 -or $r.Height -lt 1) { throw 'Control has no usable visible bounds' }
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2))|Out-Null
 [ArgonInteraction]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 700
}
function Capture($name) {
 [ArgonInteraction]::SetForegroundWindow($script:proc.MainWindowHandle) | Out-Null
 Start-Sleep -Milliseconds 500
 $r=New-Object ArgonInteraction+RECT
 $hr=[ArgonInteraction]::DwmGetWindowAttribute($script:proc.MainWindowHandle,9,[ref]$r,16)
 if ($hr -ne 0) { throw 'Window bounds unavailable' }
 $bmp=New-Object System.Drawing.Bitmap ($r.Right-$r.Left),($r.Bottom-$r.Top)
 $g=[System.Drawing.Graphics]::FromImage($bmp)
 $g.CopyFromScreen($r.Left,$r.Top,0,0,$bmp.Size)
 $bmp.Save("$work/$name",[System.Drawing.Imaging.ImageFormat]::Png)
 $g.Dispose();$bmp.Dispose()
}
try {
 Start-Argon
 Write-Host 'JS gate: actual document.title mutation observed'
 Click-Element (Find-Named 'Save page')
 $saved=(Get-Content $config -Raw|ConvertFrom-Json).saved_pages
 if ($saved.Count -ne 1 -or $saved[0].url -ne $url) { throw 'Explicit saved page did not persist' }
 Click-Element (Find-Named 'Settings')
 Click-Element (Find-Named 'Light')
 if (-not (Test-Path $config)) {
  Capture 'argon-settings-selection.png'
  Get-ChildItem $env:APPDATA,$env:LOCALAPPDATA -Filter settings.json -Recurse -ErrorAction SilentlyContinue | Select-Object -First 20 FullName | Write-Host
  throw 'Settings file missing after actual Light selection'
 }
 if ((Get-Content $config -Raw|ConvertFrom-Json).dark_theme -ne $false) { throw 'Light selection did not persist' }
 Capture 'argon-settings-light.png'
 Click-Element (Find-Named 'Dark (Zen)')
 if ((Get-Content $config -Raw|ConvertFrom-Json).dark_theme -ne $true) { throw 'Dark selection did not persist' }
 Click-Element (Find-Named 'Compact tab sidebar')
 if ((Get-Content $config -Raw|ConvertFrom-Json).sidebar_compact -ne $true) { throw 'Compact selection did not persist' }
 # Edit the homepage through its actual visible text field, then inspect
 # persistent state and exercise Home from the context menu after restart.
 $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Edit)
 $edits = $script:root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$cond)
 $homepageEdit = $null
 foreach ($e in $edits) {
  $vp = $null
  if ($e.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$vp)) {
   if ($vp.Current.Value -eq 'https://servo.org') { $homepageEdit=$e; break }
  }
 }
 if (-not $homepageEdit) { throw 'Homepage edit control not identified by current value' }
 # TextEdit exposes ValuePattern for reading but does not apply SetValue.
 # Attach input threads just for activation, then verify the real foreground
 # before sending physical keyboard events to the focused homepage edit.
 $target=$script:proc.MainWindowHandle
 $foreground=[ArgonInteraction]::GetForegroundWindow()
 $current=[ArgonInteraction]::GetCurrentThreadId()
 $other=[ArgonInteraction]::GetWindowThreadProcessId($foreground,[IntPtr]::Zero)
 try {
  [ArgonInteraction]::AttachThreadInput($current,$other,$true) | Out-Null
  [ArgonInteraction]::SetForegroundWindow($target) | Out-Null
 } finally { [ArgonInteraction]::AttachThreadInput($current,$other,$false) | Out-Null }
 $homepageEdit.SetFocus()
 Start-Sleep -Milliseconds 400
 if ([ArgonInteraction]::GetForegroundWindow() -ne $target) { throw 'Cannot type safely: Argon is not the foreground window' }
 Capture 'argon-settings-before-edit.png'
 [System.Windows.Forms.SendKeys]::SendWait('^a')
 [System.Windows.Forms.SendKeys]::SendWait('about:blank')
 [System.Windows.Forms.SendKeys]::SendWait('{TAB}')
 $valuePattern=$homepageEdit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
 Start-Sleep 1
 if ((Get-Content $config -Raw|ConvertFrom-Json).homepage -ne 'about:blank') {
  Capture 'argon-settings-homepage-failure.png'
  Write-Host (Get-Content $config -Raw)
  Write-Host "Edit accessible value: $($valuePattern.Current.Value)"
  throw 'Homepage edit did not persist'
 }
 Capture 'argon-settings-compact.png' 
 Stop-Process -Id $script:proc.Id -Force
 Start-Argon
 if ((Get-Content $config -Raw|ConvertFrom-Json).sidebar_compact -ne $true) { throw 'Settings lost across restart' }
 $saved=(Get-Content $config -Raw|ConvertFrom-Json).saved_pages
 if ($saved.Count -ne 1 -or $saved[0].url -ne $url) { throw 'Saved page lost after restart' }
 Click-Element (Find-Named 'Remove saved page')
 if (@((Get-Content $config -Raw|ConvertFrom-Json).saved_pages).Count -ne 0) { throw 'Saved page removal did not persist' }
 Capture 'argon-settings-restarted.png' 
 # Open the page menu using a physical secondary click within the app frame.
 $r=$script:root.Current.BoundingRectangle
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2))|Out-Null
 [ArgonInteraction]::mouse_event(8,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::mouse_event(16,0,0,0,[UIntPtr]::Zero)
 # Cursor motion supplies a redraw for the old candidate's menu-open path,
 # which only sets egui's internal repaint bit after intercepting the click.
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2+3),[int]($r.Y+$r.Height/2+3))|Out-Null
 Start-Sleep 1
 Capture 'argon-context-menu.png' 
 Click-Element (Find-Named 'Copy address')
 $clip=Get-Clipboard
 if ($clip -ne $url) { throw "Copy address did not copy the active URL: $clip" }
 # Open the menu again and activate Home, then Copy address confirms the
 # navigation outcome rather than merely trusting the queued action.
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2))|Out-Null
 [ArgonInteraction]::mouse_event(8,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::mouse_event(16,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2+3),[int]($r.Y+$r.Height/2+3))|Out-Null
 Start-Sleep 1
 Click-Element (Find-Named 'Home')
 Start-Sleep 3
 [ArgonInteraction]::mouse_event(8,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::mouse_event(16,0,0,0,[UIntPtr]::Zero)
 [ArgonInteraction]::SetCursorPos([int]($r.X+$r.Width/2+3),[int]($r.Y+$r.Height/2+3))|Out-Null
 Start-Sleep 1
 Click-Element (Find-Named 'Copy address')
 if ((Get-Clipboard) -ne 'about:blank') { throw 'Home did not navigate to the saved homepage' }
 Capture 'argon-settings-home-navigation.png'
 Write-Host 'Interaction gates passed: JS execution, theme and compact settings persistence across restart, homepage editing and Home navigation, right-click menu and Copy address' 
} finally {
 if ($script:proc -and -not $script:proc.HasExited) { Stop-Process -Id $script:proc.Id -Force -ErrorAction SilentlyContinue }
 # Keep later media/chrome checks isolated from this test's saved preferences.
 Remove-Item $config -ErrorAction SilentlyContinue
}
