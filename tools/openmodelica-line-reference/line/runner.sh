#!/bin/sh
set -eu

case "${MODEL:-}" in
  Line) wrapper=/reference/LinePilot.mo ;;
  FlagControl) wrapper=/reference/LineFlagPilot.mo ;;
  *) printf '%s\n' 'MODEL must be Line or FlagControl' >&2; exit 2 ;;
esac

umask 077
ulimit -f 131072
mkdir -p "$HOME"
cp "$wrapper" "$HOME/LinePilot.mo"
cat > "$HOME/run.mos" <<'EOF'
print("omc_version=" + getVersion() + "\n");
if not loadFile("/sources/modelica/Complex.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
if not loadFile("/sources/modelica/ModelicaServices/package.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
if not loadFile("/sources/modelica/Modelica/package.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
if not loadFile("/sources/buildings/Buildings/package.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
print("line_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Reals.Line) + "\n");
print("constant_source=" + getSourceFile(Modelica.Blocks.Sources.Constant) + "\n");
print("time_table_source=" + getSourceFile(Modelica.Blocks.Sources.TimeTable) + "\n");
print("Modelica=" + getVersion(Modelica) + "\n");
print("Buildings=" + getVersion(Buildings) + "\n");
if not loadFile("/tmp/home/LinePilot.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
result := simulate(
  LinePilot,
  startTime=0.0,
  stopTime=300.0,
  numberOfIntervals=5,
  method="dassl",
  tolerance=1e-9,
  fileNamePrefix="LinePilot",
  outputFormat="csv",
  variableFilter="^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$",
  simflags="");
print(getErrorString());
if not regularFileExists("LinePilot_res.csv") then
  exit(1);
end if;
exit(0);
EOF

printf 'output_directory_token=%s\n' "$OUTPUT_DIRECTORY_TOKEN"
printf 'selected_model=%s\n' "$MODEL"
printf 'container_architecture=%s\n' "$(uname -m)"
printf 'container_identity=%s\n' "$(id -u):$(id -g)"
printf 'modelica_path=%s\n' "${MODELICAPATH-unset}"
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
if ! omc "$HOME/run.mos" > "$HOME/omc.log"; then
  cat "$HOME/omc.log"
  exit 1
fi
cat "$HOME/omc.log"
grep -Fq 'resultFile = "/out/LinePilot_res.csv",' "$HOME/omc.log"
grep -Fq 'The simulation finished successfully.' "$HOME/omc.log"
warning_count=$(grep -i -c 'warning' "$HOME/omc.log" || true)
test "$warning_count" -eq 0
printf 'omc_warning_count=%s\n' "$warning_count"
test -f LinePilot_res.csv
test "$(wc -c < LinePilot_res.csv)" -le 67108864
output_kib=$(du -sk /out | cut -f1)
test "$output_kib" -le 262144
printf 'output_directory_kib=%s\n' "$output_kib"
printf 'raw_sha256='
sha256sum LinePilot_res.csv | cut -d' ' -f1
printf 'runner_complete=1\n'
touch /out/.oce-complete
while :; do sleep 1; done
