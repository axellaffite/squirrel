plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
}

kotlin {
    jvmToolchain(libs.versions.jvm.toolchain.get().toInt())
}

android {
    namespace = "app.squirrel.android"
    compileSdk = libs.versions.android.compileSdk.get().toInt()

    defaultConfig {
        applicationId = "app.squirrel.android"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.targetSdk.get().toInt()
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    // The release key stays outside the repository: its path and passwords are Gradle
    // properties (`~/.gradle/gradle.properties`, or `ORG_GRADLE_PROJECT_<name>` in the
    // environment). Without them the release build is left unsigned.
    val keystore = providers.gradleProperty("SQUIRREL_KEYSTORE").orNull
    if (keystore != null) {
        signingConfigs {
            create("release") {
                storeFile = file(keystore)
                storePassword = providers.gradleProperty("SQUIRREL_KEYSTORE_PASSWORD").get()
                keyAlias = providers.gradleProperty("SQUIRREL_KEY_ALIAS").get()
                keyPassword = providers.gradleProperty("SQUIRREL_KEY_PASSWORD").get()
            }
        }
        buildTypes {
            release {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(project(":ui"))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.androidx.work)
    // LS-DO (an agent lists and drives the screens): debug builds only, see checkReleaseHasNoLsDo.
    debugImplementation(libs.lsdo.runtime.compose)

    testImplementation(libs.kotlin.test)
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.ext.junit)
    testImplementation(libs.androidx.work.testing)
    testImplementation(libs.coroutines.test)

    androidTestImplementation(libs.kotlin.test)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.coroutines.test)
    androidTestImplementation(libs.sqldelight.android)
}

// The LS-DO runtime opens a server that drives the app: it must never reach a release build.
val checkReleaseHasNoLsDo by tasks.registering {
    group = "verification"
    description = "Fails if the LS-DO runtime is in the release runtime dependency graph."
    val release = configurations.named("releaseRuntimeClasspath").flatMap { it.incoming.resolutionResult.rootComponent }
    doLast {
        val seen = mutableSetOf<org.gradle.api.artifacts.result.ResolvedComponentResult>()
        fun walk(component: org.gradle.api.artifacts.result.ResolvedComponentResult) {
            if (!seen.add(component)) return
            component.dependencies
                .filterIsInstance<org.gradle.api.artifacts.result.ResolvedDependencyResult>()
                .forEach { walk(it.selected) }
        }
        walk(release.get())
        val leaked = seen.map { it.id.displayName }.filter { "lsdo-runtime" in it }
        check(leaked.isEmpty()) { "LS-DO runtime in the release build: $leaked" }
    }
}
tasks.named("check") { dependsOn(checkReleaseHasNoLsDo) }
tasks.matching { it.name == "assembleRelease" || it.name == "bundleRelease" }.configureEach { dependsOn(checkReleaseHasNoLsDo) }
