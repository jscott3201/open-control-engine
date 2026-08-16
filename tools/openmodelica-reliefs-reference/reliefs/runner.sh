#!/bin/sh
set -eu

case "${MODEL:-}" in
  Reliefs) wrapper=/reference/ReliefsPilot.mo ;;
  ParameterControl) wrapper=/reference/ReliefsParameterPilot.mo ;;
  FinalClamp) wrapper=/reference/ReliefsClampPilot.mo ;;
  *) printf '%s\n' 'MODEL must be Reliefs, ParameterControl, or FinalClamp' >&2; exit 2 ;;
esac

umask 077
ulimit -f 131072
mkdir -p "$HOME"
cp "$wrapper" "$HOME/ReliefsPilot.mo"
cat > "$HOME/run.mos" <<'EOF'
print("omc_version=" + getVersion() + "\n");
if not loadFile("/sources/modelica/Complex.mo", uses=false) then print(getErrorString()); exit(1); end if;
if not loadFile("/sources/modelica/ModelicaServices/package.mo", uses=false) then print(getErrorString()); exit(1); end if;
if not loadFile("/sources/modelica/Modelica/package.mo", uses=false) then print(getErrorString()); exit(1); end if;
if not loadFile("/sources/buildings/Buildings/package.mo", uses=false) then print(getErrorString()); exit(1); end if;
print("reliefs_source=" + getSourceFile(Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs) + "\n");
print("line_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Reals.Line) + "\n");
print("min_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Reals.Min) + "\n");
print("max_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Reals.Max) + "\n");
print("cdl_constant_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Reals.Sources.Constant) + "\n");
print("real_input_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Interfaces.RealInput) + "\n");
print("real_output_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Interfaces.RealOutput) + "\n");
print("msl_constant_source=" + getSourceFile(Modelica.Blocks.Sources.Constant) + "\n");
print("time_table_source=" + getSourceFile(Modelica.Blocks.Sources.TimeTable) + "\n");
print("Modelica=" + getVersion(Modelica) + "\n");
print("Buildings=" + getVersion(Buildings) + "\n");
if not loadFile("/tmp/home/ReliefsPilot.mo", uses=false) then print(getErrorString()); exit(1); end if;
result := simulate(
  ReliefsPilot,
  startTime=0.0,
  stopTime=420.0,
  numberOfIntervals=7,
  method="dassl",
  tolerance=1e-9,
  fileNamePrefix="ReliefsPilot",
  outputFormat="csv",
  variableFilter="^(uTSup|uOutDam_min|uOutDam_max|uRetDam_min|uRetDam_max|yOutDam|yRetDam)$",
  simflags="");
print(getErrorString());
if not regularFileExists("ReliefsPilot_res.csv") then exit(1); end if;
exit(0);
EOF

printf 'output_directory_token=%s\n' "$OUTPUT_DIRECTORY_TOKEN"
printf 'selected_model=%s\n' "$MODEL"
printf 'container_architecture=%s\n' "$(uname -m)"
printf 'container_identity=%s\n' "$(id -u):$(id -g)"
printf 'modelica_path=%s\n' "${MODELICAPATH-unset}"
printf 'events_enabled=default_true\n'
printf 'simflags=empty\n'
printf 'root_write_probe='
if touch /oce-root-write-probe 2>/dev/null; then printf 'unexpectedly-writable\n'; exit 1; else printf 'read-only\n'; fi
printf 'source_write_probe='
if touch /sources/oce-source-write-probe 2>/dev/null; then printf 'unexpectedly-writable\n'; exit 1; else printf 'read-only\n'; fi
printf 'reference_write_probe='
if touch /reference/oce-reference-write-probe 2>/dev/null; then printf 'unexpectedly-writable\n'; exit 1; else printf 'read-only\n'; fi
printf 'network_route_lines=%s\n' "$(wc -l < /proc/net/route)"
printf 'cgroup_memory_max=%s\n' "$(cat /sys/fs/cgroup/memory.max)"
printf 'cgroup_pids_max=%s\n' "$(cat /sys/fs/cgroup/pids.max)"
printf 'cgroup_cpu_max=%s\n' "$(cat /sys/fs/cgroup/cpu.max)"
printf 'per_file_limit_bytes=67108864\n'
printf 'output_directory_limit_bytes=268435456\n'
printf 'gcc_version=%s\n' "$(gcc -dumpfullversion)"
printf 'binutils_version=%s\n' "$(ld --version | sed -n '1s/.* //p')"
printf 'glibc_version=%s\n' "$(ldd --version | sed -n '1s/.* //p')"

cd /out
if ! omc "$HOME/run.mos" > "$HOME/omc.log"; then cat "$HOME/omc.log"; exit 1; fi
cat "$HOME/omc.log"
printf 'observed_cgroup_peak_bytes=%s\n' "$(cat /sys/fs/cgroup/memory.peak)"
grep -Fq 'resultFile = "/out/ReliefsPilot_res.csv",' "$HOME/omc.log"
grep -Fq 'The simulation finished successfully.' "$HOME/omc.log"
warning_count=$(grep -i -c 'warning' "$HOME/omc.log" || true)
test "$warning_count" -eq 0
printf 'omc_warning_count=%s\n' "$warning_count"
test -f ReliefsPilot_res.csv
test "$(wc -c < ReliefsPilot_res.csv)" -le 67108864
output_kib=$(du -sk /out | cut -f1)
test "$output_kib" -le 262144
printf 'output_directory_kib=%s\n' "$output_kib"
printf 'raw_sha256='
sha256sum ReliefsPilot_res.csv | cut -d' ' -f1
printf 'runner_complete=1\n'
touch /out/.oce-complete
while :; do sleep 1; done
