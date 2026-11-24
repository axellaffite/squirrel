plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.compose.multiplatform)
    alias(libs.plugins.android.kmp.library)
}

val iosEnabled = providers.gradleProperty("squirrel.ios").map(String::toBoolean).getOrElse(false)

kotlin {
    jvmToolchain(libs.versions.jvm.toolchain.get().toInt())

    jvm()
    android {
        namespace = "app.squirrel.ui"
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
    }
    if (iosEnabled) {
        listOf(iosArm64(), iosSimulatorArm64()).forEach { target ->
            target.binaries.framework {
                baseName = "SquirrelKit"
                isStatic = true
                export(project(":shared"))
                export(project(":theme"))
            }
        }
    }

    sourceSets {
        commonMain.dependencies {
            api(project(":shared"))
            api(project(":theme"))
            api(project(":lsdo"))
            implementation(libs.jb.lifecycle.viewmodel.compose)
            implementation(libs.jb.lifecycle.runtime.compose)
            api(libs.coil.compose)
            implementation(libs.coil.network.ktor3)
        }
        androidMain.dependencies {
            implementation(libs.androidx.activity.compose)
        }
        commonTest.dependencies {
            implementation(project(":lsdo-runtime"))
            implementation(libs.kotlin.test)
            implementation(libs.coroutines.test)
            implementation(libs.ktor.client.mock)
            implementation(libs.turbine)
        }
        jvmTest.dependencies {
            implementation(libs.sqldelight.jvm)
        }
    }
}
