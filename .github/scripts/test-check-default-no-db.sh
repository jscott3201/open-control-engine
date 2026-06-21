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
  members="$tmp/$name.members"
  trees="$tmp/$name.trees"
  metadata="$tmp/$name.metadata"
  shift 2

  mkdir -p "$trees"
  write_clean_metadata "$metadata"
  "$@" "$members" "$trees" "$metadata"

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
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac
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
  ]
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

run_fixture positive pass positive_fixture
run_fixture forbidden-non-facade fail forbidden_non_facade_fixture
run_fixture empty-members fail empty_members_fixture
run_fixture missing-tree fail missing_tree_fixture
run_fixture garbled-tree fail garbled_tree_fixture
run_fixture direct-tokio-metadata fail direct_tokio_metadata_fixture
run_fixture direct-verification-metadata fail direct_verification_metadata_fixture
run_fixture dev-verification-metadata pass dev_verification_metadata_fixture
run_fixture transitive-verification-metadata fail transitive_verification_metadata_fixture
run_fixture empty-metadata fail empty_metadata_fixture
run_fixture malformed-metadata fail malformed_metadata_fixture

echo "OK: check-default-no-db gate fixtures passed."
