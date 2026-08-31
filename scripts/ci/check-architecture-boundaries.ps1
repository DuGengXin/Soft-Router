$rules = Get-Content -Raw (Join-Path $PSScriptRoot "architecture-rules.json") | ConvertFrom-Json
$root = Resolve-Path (Join-Path $PSScriptRoot "../..")
$metadata = cargo metadata --format-version 1 --manifest-path (Join-Path $root "Cargo.toml") --no-deps | ConvertFrom-Json
if (-not $metadata) { Write-Output "SKIPPED"; exit 0 }
$failed = $false
foreach ($pkg in $metadata.packages) {
  if (-not $rules.allowed.PSObject.Properties.Name.Contains($pkg.name) -and $pkg.name -like "gateway-*") { continue }
  $allowed = @($rules.allowed.$($pkg.name))
  if ($null -eq $allowed) { continue }
  foreach ($dep in $pkg.dependencies) {
    if ($dep.name -like "gateway-*" -and $allowed -notcontains $dep.name) {
      Write-Error "$($pkg.name) must not depend on $($dep.name)"
      $failed = $true
    }
  }
}
if ($failed) { exit 1 }
Write-Output "PASSED"
exit 0
