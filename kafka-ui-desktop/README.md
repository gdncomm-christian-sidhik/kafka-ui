# Kafka UI Desktop

Runs kafka-ui as a native desktop window built with [Tauri](https://tauri.app). There is no server to deploy and you don't open it in a browser.

On launch, the app:

1. starts the bundled backend (`kafka-ui-api.jar` on a trimmed Java runtime made with `jlink`) on a free port bound to `127.0.0.1`;
2. shows a splash screen until the backend responds, then loads the UI in the window;
3. stops the backend when the window closes.

## Build

You need Rust ([rustup.rs](https://rustup.rs)), JDK 17+ with `jlink`, and the Tauri Linux dependencies (`webkit2gtk-4.1`, `libsoup3`). Node isn't needed, because the Maven `prod` profile downloads its own copy.

```sh
./build.sh             # creates deb / rpm / AppImage in src-tauri/target/release/bundle/
./build.sh --dev       # runs the app in dev mode
SKIP_JAR=1 ./build.sh  # reuses the existing backend jar
BUNDLES=appimage ./build.sh  # builds only the AppImage (comma-separated: deb,rpm,appimage)
```

## Configuration

Clusters you add in the UI are saved to `dynamic_config.yaml` in the app config directory (`~/.config/com.provectus.kafkaui.desktop/` on Linux). If you'd rather configure by hand, put an `application.yml` in that same directory and the app will load it.

Environment overrides:

| Variable             | Effect                                   |
|----------------------|------------------------------------------|
| `KAFKA_UI_JAR`       | Use a different backend jar              |
| `KAFKA_UI_JAVA_OPTS` | Extra JVM options, e.g. `-Xmx1g`         |

The backend log is written to `~/.local/share/com.provectus.kafkaui.desktop/logs/backend.log`.
