#!/usr/bin/env bash
# Gate-behavior fixtures for check-default-no-db.sh. These run without Cargo network/build work by
# injecting workspace-member, cargo-tree, and cargo-metadata fixtures through the script's test-only
# env vars.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-default-no-db.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

run_fixture() {
  name="$1"
  expected="$2"
  fixture="$3"
  expected_message="${4:-}"
  members="$tmp/$name.members"
  trees="$tmp/$name.trees"
  metadata="$tmp/$name.metadata"

  mkdir -p "$trees"
  write_clean_metadata "$metadata"
  "$fixture" "$members" "$trees" "$metadata"

  set +e
  output="$(
    OCE_NO_DB_MEMBERS_FILE="$members" \
    OCE_NO_DB_TREE_DIR="$trees" \
    OCE_NO_DB_METADATA_FILE="$metadata" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: fixture '$name' failed without a FAIL line"
        printf '%s\n' "$output"
        exit 1
      fi
      if [ -n "$expected_message" ] && ! printf '%s\n' "$output" | grep -Fq -- "$expected_message"; then
        echo "FAIL: fixture '$name' failed without expected message substring: $expected_message"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac
}

run_live_cargo_fixture() {
  name="$1"
  expected="$2"
  fixture="$3"
  expected_message="${4:-}"
  members="$tmp/$name.live.members"
  trees="$tmp/$name.live.trees"
  shim_dir="$tmp/$name.path"
  metadata="$tmp/$name.live.metadata"

  mkdir -p "$trees" "$shim_dir"
  write_clean_metadata "$metadata"
  "$fixture" "$members" "$trees" "$shim_dir" "$metadata"

  set +e
  output="$(
    unset OCE_NO_DB_METADATA_FILE
    OCE_NO_DB_MEMBERS_FILE="$members" \
    OCE_NO_DB_TREE_DIR="$trees" \
    PATH="$shim_dir:$PATH" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: live-cargo fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: live-cargo fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: live-cargo fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: live-cargo fixture '$name' failed without a FAIL line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac

  if [ -n "$expected_message" ] && ! printf '%s\n' "$output" | grep -Fq -- "$expected_message"; then
    echo "FAIL: live-cargo fixture '$name' output lacked expected substring: $expected_message"
    printf '%s\n' "$output"
    exit 1
  fi
}

write_clean_metadata() {
  metadata="$1"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "oce-model", "kind": null }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "oce-store", "kind": null }
      ]
    },
    {
      "name": "oce-model",
      "id": "path+file:///repo/crates/oce-model#0.0.0",
      "publish": null,
      "dependencies": []
    },
    {
      "name": "oce-store",
      "id": "path+file:///repo/crates/oce-store#0.0.0",
      "publish": null,
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0",
    "path+file:///repo/crates/oce-model#0.0.0",
    "path+file:///repo/crates/oce-store#0.0.0"
  ],
  "resolve": { "nodes": [] }
}
EOF
}

positive_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api oce-docs > "$members"
  cat > "$trees/oce-api.tree" <<'EOF'
oce-api v0.0.0 (/repo/crates/oce-api)
├── oce-model v0.0.0 (/repo/crates/oce-model)
└── thiserror v2.0.0
EOF
  cat > "$trees/oce-docs.tree" <<'EOF'
oce-docs v0.0.0 (/repo/crates/oce-docs)
└── oce-store v0.0.0 (/repo/crates/oce-store)
EOF
}

forbidden_non_facade_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api oce-conformance > "$members"
  cat > "$trees/oce-api.tree" <<'EOF'
oce-api v0.0.0 (/repo/crates/oce-api)
└── oce-model v0.0.0 (/repo/crates/oce-model)
EOF
  cat > "$trees/oce-conformance.tree" <<'EOF'
oce-conformance v0.0.0 (/repo/crates/oce-conformance)
└── tokio v1.0.0
EOF
}

empty_members_fixture() {
  members="$1"
  _trees="$2"
  : > "$members"
}

missing_tree_fixture() {
  members="$1"
  _trees="$2"
  printf '%s\n' oce-api > "$members"
}

garbled_tree_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api > "$members"
  printf '%s\n' 'not a cargo tree' > "$trees/oce-api.tree"
}

direct_tokio_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "tokio", "kind": "normal" }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0"
  ]
}
EOF
}

direct_tokio_macros_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "tokio-macros", "kind": "normal" }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0"
  ]
}
EOF
}

direct_verification_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "oce-reference-wal-adapter", "kind": "normal" }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    },
    {
      "name": "oce-reference-wal-adapter",
      "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
      "publish": [],
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0",
    "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0"
  ],
  "resolve": {
    "nodes": [
      {
        "id": "path+file:///repo/crates/oce-api#0.0.0",
        "deps": [
          {
            "pkg": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
            "dep_kinds": [
              { "kind": "dev" }
            ]
          }
        ]
      },
      {
        "id": "path+file:///repo/crates/oce-docs#0.0.0",
        "deps": []
      },
      {
        "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
        "deps": []
      }
    ]
  }
}
EOF
}

dev_verification_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "oce-reference-wal-adapter", "kind": "dev" }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    },
    {
      "name": "oce-reference-wal-adapter",
      "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
      "publish": [],
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0",
    "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0"
  ],
  "resolve": {
    "nodes": [
      {
        "id": "path+file:///repo/crates/oce-api#0.0.0",
        "deps": [
          {
            "pkg": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
            "dep_kinds": [
              { "kind": "dev" }
            ]
          }
        ]
      },
      {
        "id": "path+file:///repo/crates/oce-docs#0.0.0",
        "deps": []
      },
      {
        "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
        "deps": []
      }
    ]
  }
}
EOF
}

transitive_verification_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-api",
      "id": "path+file:///repo/crates/oce-api#0.0.0",
      "publish": null,
      "dependencies": [
        { "name": "third-party-helper", "kind": "normal" }
      ]
    },
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    },
    {
      "name": "oce-reference-wal-adapter",
      "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
      "publish": [],
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-api#0.0.0",
    "path+file:///repo/crates/oce-docs#0.0.0",
    "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0"
  ],
  "resolve": {
    "nodes": [
      {
        "id": "path+file:///repo/crates/oce-api#0.0.0",
        "deps": [
          {
            "pkg": "registry+https://github.com/rust-lang/crates.io-index#third-party-helper@1.0.0",
            "dep_kinds": [
              { "kind": "normal" }
            ]
          }
        ]
      },
      {
        "id": "registry+https://github.com/rust-lang/crates.io-index#third-party-helper@1.0.0",
        "deps": [
          {
            "pkg": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
            "dep_kinds": [
              { "kind": "normal" }
            ]
          }
        ]
      },
      {
        "id": "path+file:///repo/crates/oce-docs#0.0.0",
        "deps": []
      },
      {
        "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
        "deps": []
      }
    ]
  }
}
EOF
}

empty_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  : > "$metadata"
}

malformed_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  printf '%s\n' 'not cargo metadata json' > "$metadata"
}

zero_shipped_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-reference-wal-adapter",
      "id": "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0",
      "publish": [],
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-reference-wal-adapter#0.0.0"
  ],
  "resolve": { "nodes": [] }
}
EOF
}

missing_facade_metadata_fixture() {
  members="$1"
  trees="$2"
  metadata="$3"
  positive_fixture "$members" "$trees"
  cat > "$metadata" <<'EOF'
{
  "packages": [
    {
      "name": "oce-docs",
      "id": "path+file:///repo/crates/oce-docs#0.0.0",
      "publish": null,
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/oce-docs#0.0.0"
  ],
  "resolve": { "nodes": [] }
}
EOF
}

live_cargo_failure_fixture() {
  members="$1"
  trees="$2"
  shim_dir="$3"
  _metadata="$4"
  positive_fixture "$members" "$trees"
  cat > "$shim_dir/cargo" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "metadata" ]; then
  echo "simulated cargo metadata stderr" >&2
  exit 42
fi
echo "unexpected cargo command: $*" >&2
exit 99
EOF
  chmod +x "$shim_dir/cargo"
}

live_cargo_warning_metadata_fixture() {
  members="$1"
  trees="$2"
  shim_dir="$3"
  metadata="$4"
  positive_fixture "$members" "$trees"
  cat > "$shim_dir/cargo" <<EOF
#!/usr/bin/env bash
if [ "\${1:-}" = "metadata" ]; then
  echo "simulated benign cargo metadata warning" >&2
  cat "$metadata"
  exit 0
fi
echo "unexpected cargo command: \$*" >&2
exit 99
EOF
  chmod +x "$shim_dir/cargo"
}

run_fixture positive pass positive_fixture
run_fixture forbidden-non-facade fail forbidden_non_facade_fixture \
  "FAIL: 'oce-conformance' default build links a database / async runtime / parallelism runtime"
run_fixture empty-members fail empty_members_fixture \
  "workspace member fixture is missing or empty"
run_fixture missing-tree fail missing_tree_fixture \
  "tree fixture for 'oce-api' is missing or empty"
run_fixture garbled-tree fail garbled_tree_fixture \
  "did not contain the expected 'oce-api v' root node"
run_fixture direct-tokio-metadata fail direct_tokio_metadata_fixture \
  "shipped crate 'oce-api' has forbidden direct normal dependency 'tokio'"
run_fixture direct-tokio-macros-metadata fail direct_tokio_macros_metadata_fixture \
  "shipped crate 'oce-api' has forbidden direct normal dependency 'tokio-macros'"
run_fixture direct-verification-metadata fail direct_verification_metadata_fixture \
  "shipped crate 'oce-api' has forbidden direct normal dependency on verification-only crate 'oce-reference-wal-adapter'"
run_fixture dev-verification-metadata pass dev_verification_metadata_fixture
run_fixture transitive-verification-metadata fail transitive_verification_metadata_fixture \
  "shipped crate 'oce-api' reaches verification-only crate 'oce-reference-wal-adapter' through non-dev dependencies"
run_fixture empty-metadata fail empty_metadata_fixture \
  "cargo metadata fixture is missing or empty"
run_fixture malformed-metadata fail malformed_metadata_fixture \
  "cargo metadata was missing a package array or was malformed"
run_fixture zero-shipped-metadata fail zero_shipped_metadata_fixture \
  "cargo metadata contained no shipped crates (publish == null)"
run_fixture missing-facade-metadata fail missing_facade_metadata_fixture \
  "cargo metadata shipped set did not include the canonical facade crate 'oce-api'"
run_live_cargo_fixture live-cargo-failure fail live_cargo_failure_fixture \
  "simulated cargo metadata stderr"
run_live_cargo_fixture live-cargo-warning pass live_cargo_warning_metadata_fixture \
  "simulated benign cargo metadata warning"

echo "OK: check-default-no-db gate fixtures passed."
