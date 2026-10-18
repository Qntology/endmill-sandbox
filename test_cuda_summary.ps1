# test_cuda.bat 가 끝난 뒤 부르는 요약 도우미.
#  - 단계 로그(NN_*.log)에서 시뮬레이션 교차 점검 줄([EOL] [MATRIX] [AUDIT] [UNKNOWN] [COAT] [TEMP] [ENGAGE] [GATES])을
#    highlights.log 한 파일로 모은다 (로그와 wear_logs 결과를 맞대 볼 때 이 파일 하나만 열면 된다).
#  - 실패한 테스트 이름, panic 위치, 컴파일 경고(링커 경고 제외), 감사 플래그 등급별 개수, 공구 수명 사건 개수,
#    N:N 행렬 판정 개수, 측정 없이는 알 수 없는 항목 수를 summary.log 끝에 덧붙인다.
#  - 이번 실행이 만든 결과 파일 목록을 manifest.txt 로 남긴다.
param([Parameter(Mandatory = $true)][string]$LogDir)
$ErrorActionPreference = 'Continue'
$enc = New-Object System.Text.UTF8Encoding($false)
$summary = Join-Path $LogDir 'summary.log'
$highlights = Join-Path $LogDir 'highlights.log'
$tag = '^\[(EOL|MATRIX|MATRIX-GRID|AUDIT|UNKNOWN|COAT|TEMP|ENGAGE|GATES)\]?'
$hl = New-Object System.Collections.Generic.List[string]
$failedTests = New-Object System.Collections.Generic.List[string]
$panics = New-Object System.Collections.Generic.List[string]
$warnings = 0
$audit = @{ '정보' = 0; '주의' = 0; '신뢰불가' = 0 }
$auditIds = @{}
$eol = @{}
$matrix = @{ '적합' = 0; '주의' = 0; '부적합' = 0 }
$unknown = @{}
$logs = Get-ChildItem -LiteralPath $LogDir -Filter '*.log' | Where-Object { $_.Name -match '^\d\d_' } | Sort-Object Name
foreach ($f in $logs) {
    $lines = [System.IO.File]::ReadAllLines($f.FullName, [System.Text.Encoding]::UTF8)
    $picked = New-Object System.Collections.Generic.List[string]
    foreach ($l in $lines) {
        if ($l -match $tag) { $picked.Add($l) }
        if ($l -match '^test (.+) \.\.\. FAILED') { $failedTests.Add("$($f.BaseName): $($Matches[1])") }
        if ($l -match 'panicked at (.+)$') { $panics.Add("$($f.BaseName): $($Matches[1])") }
        if ($l -match '^warning: ' -and $l -notmatch 'linker stdout' -and $l -notmatch 'generated \d+ warning') { $warnings++ }
        if ($l -match '^\[AUDIT\]\[(정보|주의|신뢰불가)\] (\S+)') {
            $audit[$Matches[1]]++
            $k = "$($Matches[1]) $($Matches[2])"
            if ($auditIds.ContainsKey($k)) { $auditIds[$k]++ } else { $auditIds[$k] = 1 }
        }
        if ($l -match '^\[EOL\].*?\b(failure|stop|tool_change|life_end|start_worn|chip_risk|load_alarm|accelerated|warn)\b\s+#') {
            $k = $Matches[1]
            if ($eol.ContainsKey($k)) { $eol[$k]++ } else { $eol[$k] = 1 }
        }
        if ($l -match '^\[MATRIX\] .* \| 판정 (적합|주의|부적합)') { $matrix[$Matches[1]]++ }
        if ($l -match '^\[UNKNOWN\] (\S+)') { $unknown[$Matches[1]] = 1 }
    }
    if ($picked.Count -gt 0) {
        $hl.Add("===== $($f.Name) ($($picked.Count) 줄) =====")
        $hl.AddRange($picked)
        $hl.Add('')
    }
}
[System.IO.File]::WriteAllLines($highlights, $hl, $enc)

$s = New-Object System.Collections.Generic.List[string]
$s.Add('')
$s.Add('======== 시뮬레이션 교차 점검 요약 (highlights.log 에 원문) ========')
$s.Add(("감사 플래그: 신뢰불가 {0} · 주의 {1} · 정보 {2}" -f $audit['신뢰불가'], $audit['주의'], $audit['정보']))
foreach ($k in ($auditIds.Keys | Where-Object { $_ -notmatch '^정보' } | Sort-Object)) { $s.Add(("  {0} ×{1}" -f $k, $auditIds[$k])) }
if ($eol.Count -gt 0) {
    $s.Add('공구 수명 사건: ' + (($eol.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name) $($_.Value)" }) -join ' · '))
}
$mt = $matrix['적합'] + $matrix['주의'] + $matrix['부적합']
if ($mt -gt 0) { $s.Add(("공구×소재 행렬: {0} 칸 · 적합 {1} · 주의 {2} · 부적합 {3}" -f $mt, $matrix['적합'], $matrix['주의'], $matrix['부적합'])) }
if ($unknown.Count -gt 0) { $s.Add(("측정 없이는 알 수 없는 항목 [UNKNOWN]: {0}종 — {1}" -f $unknown.Count, (($unknown.Keys | Sort-Object) -join ', '))) }
$s.Add(("컴파일 경고 (링커 경고 제외): {0}" -f $warnings))
if ($failedTests.Count -gt 0) { $s.Add('실패한 테스트:'); foreach ($t in $failedTests) { $s.Add("  $t") } }
if ($panics.Count -gt 0) { $s.Add('panic 위치:'); foreach ($p in ($panics | Select-Object -First 20)) { $s.Add("  $p") } }
[System.IO.File]::AppendAllLines($summary, $s, $enc)

$files = Get-ChildItem -LiteralPath $LogDir -Recurse -File | Sort-Object FullName | ForEach-Object {
    "{0,10:N0}  {1}" -f $_.Length, $_.FullName.Substring($LogDir.Length).TrimStart('\')
}
[System.IO.File]::WriteAllLines((Join-Path $LogDir 'manifest.txt'), [string[]]$files, $enc)
