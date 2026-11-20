param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string[]] $Path
)

$ErrorActionPreference = 'Stop'

function Percentile95($Values) {
    $ordered = @($Values | Sort-Object)
    if ($ordered.Count -eq 0) { return $null }
    return $ordered[[int][Math]::Ceiling(0.95 * $ordered.Count) - 1]
}

function Maximum($Values) {
    $valuesArray = @($Values)
    if ($valuesArray.Count -eq 0) { return $null }
    return ($valuesArray | Measure-Object -Maximum).Maximum
}

foreach ($file in $Path) {
    $report = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
    if ($report.version -ne 1 -or $report.truncated_events -ne 0) {
        throw "Unsupported or truncated measurement file: $file"
    }
    $editing = @($report.events | Where-Object { $_.event -eq 'caption_edit_dispatch' } |
        ForEach-Object { $_.us })
    $timer = @($report.events | Where-Object { $_.event -eq 'gui_timer_interval' } |
        ForEach-Object { $_.us })
    $resume = @($report.events | Where-Object { $_.event -eq 'resume_paint' } |
        ForEach-Object { [Math]::Abs([long]$_.video_pts_us - [long]$_.audio_queue_position_us) })
    $resumeSubmit = @($report.events | Where-Object { $_.event -eq 'resume_frame_submit' } |
        ForEach-Object { [Math]::Abs([long]$_.video_pts_us - [long]$_.audio_requested_position_us) })
    foreach ($proxy in @($false, $true)) {
        $inputs = @($report.events | Where-Object { $_.event -eq 'scrub_input' -and $_.proxy -eq $proxy })
        if ($inputs.Count -eq 0) { continue }
        $paints = @($report.events | Where-Object { $_.event -eq 'scrub_paint' -and $_.proxy -eq $proxy })
        $submits = @($report.events | Where-Object { $_.event -eq 'scrub_frame_submit' -and $_.proxy -eq $proxy })
        [pscustomobject]@{
            File = $file
            Proxy = $proxy
            ScrubInputs = $inputs.Count
            ScrubPaints = $paints.Count
            ScrubPaintP95Us = Percentile95 @($paints | ForEach-Object { $_.us })
            ScrubMediaErrorMaxUs = Maximum @($paints | ForEach-Object {
                [Math]::Abs([long]$_.video_pts_us - [long]$_.target_us)
            })
            ScrubFrameSubmits = $submits.Count
            ScrubFrameSubmitP95Us = Percentile95 @($submits | ForEach-Object { $_.us })
            ScrubFrameSubmitMediaErrorMaxUs = Maximum @($submits | ForEach-Object {
                [Math]::Abs([long]$_.video_pts_us - [long]$_.target_us)
            })
            ResumeSubmitOffsetP95Us = Percentile95 $resumeSubmit
            ResumeInternalAvP95Us = Percentile95 $resume
            CaptionDispatchP95Us = Percentile95 $editing
            GuiTimerP95Us = Percentile95 $timer
            GuiTimerMaxUs = Maximum $timer
        }
    }
}
