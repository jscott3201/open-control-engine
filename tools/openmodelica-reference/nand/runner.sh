#!/bin/sh
set -eu

case "${MODEL:-}" in
  Nand) wrapper=/reference/NandPilot.mo ;;
  And) wrapper=/reference/AndPilot.mo ;;
  *) printf '%s\n' 'MODEL must be Nand or And' >&2; exit 2 ;;
esac

umask 077
ulimit -f 131072
mkdir -p "$HOME"
cp "$wrapper" "$HOME/NandPilot.mo"
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
print("nand_source=" + getSourceFile(Buildings.Controls.OBC.CDL.Logical.Nand) + "\n");
print("boolean_table_source=" + getSourceFile(Modelica.Blocks.Sources.BooleanTable) + "\n");
print("Modelica=" + getVersion(Modelica) + "\n");
print("Buildings=" + getVersion(Buildings) + "\n");
if not loadFile("/tmp/home/NandPilot.mo", uses=false) then
  print(getErrorString());
  exit(1);
end if;
result := simulate(
  NandPilot,
  startTime=0.0,
  stopTime=240.0,
  numberOfIntervals=4,
  method="dassl",
  tolerance=1e-9,
  fileNamePrefix="NandPilot",
  outputFormat="csv",
  variableFilter=".*");
print(getErrorString());
if not regularFileExists("NandPilot_res.csv") then
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
grep -Fq 'resultFile = "/out/NandPilot_res.csv",' "$HOME/omc.log"
grep -Fq 'The simulation finished successfully.' "$HOME/omc.log"
test -f NandPilot_res.csv
test "$(wc -c < NandPilot_res.csv)" -le 67108864
output_kib=$(du -sk /out | cut -f1)
test "$output_kib" -le 262144
printf 'output_directory_kib=%s\n' "$output_kib"
printf 'raw_sha256='
sha256sum NandPilot_res.csv | cut -d' ' -f1
printf 'runner_complete=1\n'
touch /out/.oce-complete
while :; do sleep 1; done
