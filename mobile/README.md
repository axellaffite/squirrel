# Squirrel phone app

Kotlin Multiplatform: one Compose UI and one backup engine for Android and iOS.

- `shared/`: the generated API client, sign-in and token refresh, the SQLite upload queue, hashing and
  the sync engine, plus the platform parts (MediaStore and WorkManager on Android; PhotoKit, the
  background URLSession and BGTaskScheduler on iOS, written in Kotlin/Native).
- `theme/`: the design system, generated from `web/src/lib/styles/tokens.css` (`MASTER.md` governs it).
- `ui/`: the shared screens, and the `SquirrelKit` framework the iOS app links.
- `androidApp/`: the Android app. `iosApp/`: the Xcode project, a thin Swift host for the Compose UI.

Design and decisions: `docs/plans/2026-11-16-phase-6-kmp-app.md`.

## Prerequisites

- **JDK 21.** Gradle's toolchain is pinned to 21 (`gradle/libs.versions.toml`, `jvm-toolchain`) and
  never downloads one (`org.gradle.java.installations.auto-download=false`): install a JDK 21
  (Temurin, for example) and let Gradle find it, or point `JAVA_HOME` at it.
- **Gradle** comes with the wrapper (`./gradlew`, Gradle 9.7.1). Kotlin 2.4.21, AGP 9.3.3,
  Compose Multiplatform 1.12.2.
- **Android SDK** with platform 37 (`compileSdk`; the app targets 36, minimum 29). Set `sdk.dir`
  in `mobile/local.properties` or `ANDROID_HOME`.
- **iOS only:** a Mac with Xcode. Kotlin 2.4.21 officially supports Xcode 26.4; the app is
  developed and tested with Xcode 27 on the iPhone 17 simulator.

Run every Gradle command from `mobile/`.

## Build and test

    ./gradlew build                          # every JVM/Android build and test
    ./gradlew :shared:checkApiClient         # the generated client still matches openapi.json
    ./gradlew :androidApp:testDebugUnitTest :androidApp:lintDebug
    ./gradlew :androidApp:assembleDebug      # androidApp/build/outputs/apk/debug/androidApp-debug.apk

The iOS targets are configured but built only when asked, so Android builds stay fast:

    ./gradlew -Psquirrel.ios=true :shared:iosSimulatorArm64Test :ui:iosSimulatorArm64Test

The native theme is generated from the web tokens; after changing `tokens.css`, run
`pnpm gen:native-tokens` in `web/` (CI runs `pnpm check:native-tokens` and fails on drift).

## Android: install the debug app

With a phone in developer mode (Settings › About phone, tap the build number seven times):

- **USB:** enable USB debugging, plug the phone in and accept the prompt, then

      ./gradlew :androidApp:installDebug
      # or: adb install -r androidApp/build/outputs/apk/debug/androidApp-debug.apk

- **Wi-Fi (Android 11+):** in Developer options › Wireless debugging, choose "Pair device with
  pairing code", then on the computer (same network):

      adb pair <phone-ip>:<pairing-port>     # enter the six-digit code
      adb connect <phone-ip>:<port>          # the port shown on the Wireless debugging screen
      ./gradlew :androidApp:installDebug

`adb devices` lists what is connected. The app is `app.squirrel.android`.

### Which address to type in the app

- **Emulator:** the emulator cannot reach the host through `10.0.2.2` behind a proxy; forward the
  port instead and use `http://localhost:<port>`:

      adb reverse tcp:8080 tcp:8080

- **Phone:** the computer's LAN address, e.g. `http://192.168.1.10:8080`. Plain HTTP is accepted only
  for home and VPN addresses (private ranges, Tailscale, `.local`/`.lan`/`.home.arpa`, loopback); any
  other server needs HTTPS. The server must listen on that interface (`SQUIRREL_BIND=0.0.0.0:8080`).

## iOS: run on the simulator

    open iosApp/iosApp.xcodeproj           # scheme iosApp, any iPhone simulator, Run

or from the command line:

    xcodebuild -project iosApp/iosApp.xcodeproj -scheme iosApp \
      -destination 'platform=iOS Simulator,name=iPhone 17' build
    xcrun simctl install booted <DerivedData>/Build/Products/Debug-iphonesimulator/iosApp.app
    xcrun simctl launch booted app.squirrel.ios

The "Compile Kotlin" build phase runs `:ui:embedAndSignAppleFrameworkForXcode -Psquirrel.ios=true`
before Swift compiles, so Xcode always links a fresh `SquirrelKit`. The simulator reaches a server on
the Mac as `http://localhost:<port>`. Test photos: `xcrun simctl addmedia booted <files>`; photo
access without the prompt: `xcrun simctl privacy booted grant photos app.squirrel.ios`.

XCTests (the Keychain store, PhotoKit, the background transport, end to end):

    xcodebuild test -project iosApp/iosApp.xcodeproj -scheme iosApp \
      -destination 'platform=iOS Simulator,name=iPhone 17'

`xcodebuild test` prints "Test Suite 'All tests' passed" and may linger for minutes afterwards;
the results are final at that line.

### A real iPhone

The project has no team set, so it runs only on the simulator until you sign it:

1. Xcode › Settings › Accounts: add your Apple Account (a free account works; apps then expire after
   7 days).
2. Target iosApp › Signing & Capabilities: keep "Automatically manage signing", choose your team,
   and change the bundle identifier if `app.squirrel.ios` is taken (it must be unique to the team).
3. On the iPhone: enable Developer Mode (Settings › Privacy & Security), connect it, select it as
   the run destination, Run. The first time, trust your developer certificate in
   Settings › General › VPN & Device Management.

Background backups need the "Background Modes" capability (fetch, processing) that the project
already declares in `Info.plist`; nothing else is needed for a development build.

## End-to-end tests against a throwaway server

`scripts/mobile-e2e-server.sh` starts a Squirrel on port 8188 with its own database on the dev
Postgres (localhost:55432, database `squirrel_mobile_e2e…`), its own data directory and one user:

    docker compose -f ../deploy/docker-compose.dev.yml up -d --wait
    scripts/mobile-e2e-server.sh start     # prints the address, user and password
    scripts/mobile-e2e-server.sh stop      # stops it and drops everything

`SQUIRREL_MOBILE_E2E_PORT`, `SQUIRREL_MOBILE_E2E_DB` (must start with `squirrel_mobile_e2e`) and
`SQUIRREL_MOBILE_E2E_STATE` override the defaults. Then:

- **Android** (emulator or phone, after `adb reverse tcp:8188 tcp:8188`):

      ./gradlew :androidApp:connectedDebugAndroidTest \
        -Pandroid.testInstrumentationRunnerArguments.squirrelServer=http://localhost:8188

- **iOS** (photos added and access granted as above):

      TEST_RUNNER_SQUIRREL_SERVER=http://localhost:8188 xcodebuild test \
        -project iosApp/iosApp.xcodeproj -scheme iosApp \
        -destination 'platform=iOS Simulator,name=iPhone 17'

Both tests skip without a server.

## Where the app keeps things

- **Refresh token:** Android, encrypted with an Android Keystore key in the app's database; iOS, the
  Keychain (`AfterFirstUnlockThisDeviceOnly`, never synchronised). The access token stays in memory.
- **Queue and settings:** the app's SQLite database (SQLDelight), forgotten at sign-out except the
  server address and backup preferences.
- **Caches:** thumbnails, and originals downloaded to open or share, are cleared at sign-out (iOS
  also clears opened files at launch).
