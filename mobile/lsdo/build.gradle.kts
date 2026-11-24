// LS-DO contract: what a screen implements to list its state and actions (ls) and run them (do).
// Ships in every build: interfaces and a registration hook only. The registry and the server that
// act on it live in :lsdo-runtime, which only debug builds include.
plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.android.kmp.library)
}

val iosEnabled = providers.gradleProperty("squirrel.ios").map(String::toBoolean).getOrElse(false)

kotlin {
    jvmToolchain(libs.versions.jvm.toolchain.get().toInt())

    jvm()
    android {
        namespace = "app.squirrel.lsdo"
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
    }
    if (iosEnabled) {
        iosArm64()
        iosSimulatorArm64()
    }

    sourceSets {
        commonMain.dependencies {
            api(libs.coroutines.core)
            api(libs.serialization.json)
        }
        commonTest.dependencies {
            implementation(libs.kotlin.test)
        }
    }
}
