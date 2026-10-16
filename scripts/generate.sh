#!/usr/bin/env bash
# Regenerate src/generated (low-level operations + models) from openapi.json with a pinned openapi-generator.
# The ergonomic client (src/lib.rs, src/models.rs, src/error.rs) is hand-written and never touched here.
#   scripts/generate.sh                 # uses ./openapi.json
#   SPEC=path/to/openapi.json scripts/generate.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OAG_VERSION=7.26.0
SPEC=${SPEC:-openapi.json}
[ "$SPEC" != openapi.json ] && cp "$SPEC" openapi.json
JAR=${OAG_JAR:-${XDG_CACHE_HOME:-$HOME/.cache}/openapi-generator/openapi-generator-cli-$OAG_VERSION.jar}
if [ ! -f "$JAR" ]; then
  mkdir -p "$(dirname "$JAR")"
  curl -sSfLo "$JAR" "https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/$OAG_VERSION/openapi-generator-cli-$OAG_VERSION.jar"
fi
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
python3 scripts/normalize-spec.py openapi.json > "$TMP/spec.json"
java -jar "$JAR" generate -g rust -i "$TMP/spec.json" -o "$TMP/out" --skip-validate-spec \
  --additional-properties=packageName=suncharts_generated,library=reqwest,supportAsync=true,hideGenerationTimestamp=true \
  --type-mappings=uuid=String,UUID=String,date=String,DateTime=String,date-time=String \
  --global-property=apiDocs=false,modelDocs=false > "$TMP/gen.log" 2>&1 || { cat "$TMP/gen.log"; exit 1; }
rm -rf src/generated && mkdir -p src/generated
cp -R "$TMP/out/src/apis" "$TMP/out/src/models" src/generated/
# The generator emits a standalone crate; mount it as `crate::generated`.
find src/generated -name '*.rs' -exec perl -pi -e 's/crate::(apis|models)/crate::generated::$1/g; s/use crate::\{/use crate::generated::{/g' {} +
cat > src/generated/mod.rs <<'RS'
//! Low-level operations and models **generated from the SunCharts OpenAPI spec** (`openapi.json`) by
//! openapi-generator. Do not edit: run `scripts/generate.sh`. Get an authenticated
//! [`apis::configuration::Configuration`] from [`crate::Client::api_config`].
#![allow(clippy::all, unused_imports, non_snake_case, missing_docs, rustdoc::all)]
pub mod apis;
pub mod models;
RS
rustfmt --edition 2021 src/generated/mod.rs
echo "generated src/generated from $(python3 -c 'import json;print(json.load(open("openapi.json"))["info"]["version"])') spec"
