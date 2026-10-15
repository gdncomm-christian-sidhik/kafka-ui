#!/usr/bin/env bash
# Builds the kafka-ui backend jar (with the React UI baked in), a trimmed Java
# runtime, and packages both into a Tauri desktop app.
#
#   ./build.sh            full build: jar + runtime + installers (deb/rpm/AppImage)
#   ./build.sh --dev      jar + runtime, then `cargo tauri dev`
#   SKIP_JAR=1 ./build.sh reuse the existing kafka-ui-api jar
#   BUNDLES=appimage ./build.sh  only build the given bundles (comma-separated: deb,rpm,appimage)
set -euo pipefail

DESKTOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$DESKTOP_DIR")"
TAURI_DIR="$DESKTOP_DIR/src-tauri"
RES_DIR="$TAURI_DIR/resources"

# Modules the backend needs; java.management.rmi for broker JMX metrics,
# java.security.jgss/jdk.security.auth for SASL/Kerberos, jdk.crypto.* for TLS.
JAVA_MODULES="java.base,java.compiler,java.desktop,java.instrument,java.logging,\
java.management,java.management.rmi,java.naming,java.net.http,java.prefs,java.rmi,\
java.scripting,java.security.jgss,java.security.sasl,java.sql,java.transaction.xa,\
java.xml,jdk.charsets,jdk.crypto.cryptoki,jdk.crypto.ec,jdk.jfr,jdk.localedata,\
jdk.management,jdk.naming.dns,jdk.net,jdk.security.auth,jdk.security.jgss,\
jdk.unsupported,jdk.zipfs"

command -v cargo >/dev/null || { echo "cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }
cargo tauri --version >/dev/null 2>&1 || cargo install tauri-cli --version "^2" --locked

if [[ "${SKIP_JAR:-0}" != 1 ]]; then
  # The project's Lombok (1.18.24) can't compile on JDK 21+; 1.18.30 can.
  (cd "$ROOT_DIR" && ./mvnw -B -ntp -Pprod -pl kafka-ui-api -am -DskipTests -Ddocker.skip=true \
    -Dorg.projectlombok.version="${LOMBOK_VERSION:-1.18.30}" package)
fi
JAR="$(ls "$ROOT_DIR"/kafka-ui-api/target/kafka-ui-api-*.jar | grep -v -- '-plain\|-sources\|-javadoc' | head -1)"
[[ -f "$JAR" ]] || { echo "backend jar not found in kafka-ui-api/target" >&2; exit 1; }

mkdir -p "$RES_DIR"
cp "$JAR" "$RES_DIR/kafka-ui-api.jar"

if [[ ! -x "$RES_DIR/runtime/bin/java" || "${REBUILD_RUNTIME:-0}" == 1 ]]; then
  rm -rf "$RES_DIR/runtime"
  jlink --add-modules "$JAVA_MODULES" \
    --strip-debug --no-header-files --no-man-pages --compress=zip-6 \
    --output "$RES_DIR/runtime"
  # jlink emits read-only files, which break re-copying during bundling.
  chmod -R u+w "$RES_DIR/runtime"
fi

if [[ ! -f "$TAURI_DIR/icons/icon.png" ]]; then
  (cd "$TAURI_DIR" && cargo tauri icon "$ROOT_DIR/kafka-ui-react-app/public/favicon/icon-512.png")
fi

cd "$TAURI_DIR"
if [[ "${1:-}" == "--dev" ]]; then
  cargo tauri dev
else
  # linuxdeploy's bundled strip fails on newer system libraries (e.g. Arch).
  # linuxdeploy must resolve libjvm.so, which the JRE normally finds via $ORIGIN.
  LD_LIBRARY_PATH="$RES_DIR/runtime/lib/server:$RES_DIR/runtime/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    NO_STRIP=true cargo tauri build ${BUNDLES:+--bundles "$BUNDLES"}
  echo
  echo "Installers: $TAURI_DIR/target/release/bundle/"
fi
