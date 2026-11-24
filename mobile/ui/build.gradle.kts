plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.compose.multiplatform)
    alias(libs.plugins.android.kmp.library)
}

val iosEnabled = providers.gradleProperty("squirrel.ios").map(String::toBoolean).getOrElse(false)

// LS-DO on iOS. Kotlin/Native compiles iosMain once for debug and release (the build type only
// exists at link time), so the switch is made here: Xcode's Debug configuration (CONFIGURATION,
// set when it runs embedAndSignAppleFrameworkForXcode), or -Psquirrel.lsdo=true|false.
val xcodeConfiguration = providers.environmentVariable("CONFIGURATION").orNull
val lsdoEnabled = providers.gradleProperty("squirrel.lsdo").map(String::toBoolean)
    .getOrElse(xcodeConfiguration == "Debug")
check(!(lsdoEnabled && xcodeConfiguration == "Release")) { "LS-DO cannot be built into a Release app (-Psquirrel.lsdo=true with CONFIGURATION=Release)." }

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
        if (iosEnabled) {
            iosMain {
                // Twins, as on Android: the real startLsDo() with the runtime, or an empty one.
                kotlin.srcDir(if (lsdoEnabled) "src/iosLsDo/kotlin" else "src/iosNoLsDo/kotlin")
                if (lsdoEnabled) dependencies { implementation(project(":lsdo-runtime")) }
            }
        }
    }
}

// Belt and braces: a release framework must not contain the LS-DO runtime, whatever the switch said.
tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinNativeLink>()
    .matching { it.name.startsWith("linkRelease") }
    .configureEach {
        val binary = outputFile
        doLast {
            val file = binary.get().let { if (it.isDirectory) it.resolve(it.nameWithoutExtension) else it }
            val leaked = "app.squirrel.lsdo.runtime" in String(file.readBytes(), Charsets.ISO_8859_1)
            check(!leaked) { "LS-DO runtime linked into the release binary ${file.name}." }
        }
    }
