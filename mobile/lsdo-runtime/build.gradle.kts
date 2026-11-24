// LS-DO runtime: the registry screens register into, and a localhost HTTP server an agent drives
// (GET /ls, POST /do/{screen}). Debug builds only: never a dependency of release code.
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
        namespace = "app.squirrel.lsdo.runtime"
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
    }
    if (iosEnabled) {
        iosArm64()
        iosSimulatorArm64()
    }

    sourceSets {
        commonMain.dependencies {
            api(project(":lsdo"))
            implementation(libs.ktor.server.core)
            implementation(libs.ktor.server.cio)
        }
        commonTest.dependencies {
            implementation(libs.kotlin.test)
            implementation(libs.coroutines.test)
        }
        jvmTest.dependencies {
            implementation(libs.ktor.server.test.host)
        }
    }
}
