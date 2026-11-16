import org.jetbrains.kotlin.gradle.tasks.KotlinCompilationTask
import org.openapitools.generator.gradle.plugin.tasks.GenerateTask

plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.android.kmp.library)
    alias(libs.plugins.openapi.generator)
}

val iosEnabled = providers.gradleProperty("squirrel.ios").map(String::toBoolean).getOrElse(false)

// The committed contract; -PopenapiSpec=<path> points the drift check at another copy.
val openapiSpec: File = providers.gradleProperty("openapiSpec")
    .map { file(it) }
    .getOrElse(rootProject.file("../openapi.json"))
val generatedApiDir = layout.buildDirectory.dir("generated/openapi")

openApiGenerate {
    generatorName.set("kotlin")
    library.set("multiplatform")
    inputSpec.set(openapiSpec.path)
    outputDir.set(generatedApiDir.get().asFile.path)
    ignoreFileOverride.set(file("openapi/.openapi-generator-ignore").path)
    packageName.set("app.squirrel.api")
    configOptions.set(
        mapOf(
            "dateLibrary" to "kotlinx-datetime",
            "enumUnknownDefaultCase" to "true",
            "omitGradleWrapper" to "true",
            "omitGradlePluginVersions" to "true",
        ),
    )
    generateApiTests.set(false)
    generateModelTests.set(false)
    generateApiDocumentation.set(false)
    generateModelDocumentation.set(false)
    // openapi.json has `info.license: {name: ""}` (utoipa), which the 3.1 validator rejects;
    // everything else validates, and generation parses the whole contract anyway.
    skipValidateSpec.set(true)
}

kotlin {
    jvmToolchain(libs.versions.jvm.toolchain.get().toInt())

    jvm()
    android {
        namespace = "app.squirrel.shared"
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
    }
    if (iosEnabled) {
        iosArm64()
        iosSimulatorArm64()
    }

    sourceSets {
        commonMain {
            kotlin.srcDir(generatedApiDir.map { it.dir("src/commonMain/kotlin") })
            dependencies {
                api(libs.coroutines.core)
                api(libs.ktor.client.core)
                api(libs.datetime)
                implementation(libs.ktor.client.auth)
                implementation(libs.ktor.client.content.negotiation)
                implementation(libs.ktor.serialization.json)
                implementation(libs.serialization.json)
            }
        }
        commonTest.dependencies {
            implementation(libs.kotlin.test)
            implementation(libs.coroutines.test)
            implementation(libs.ktor.client.mock)
            implementation(libs.turbine)
        }
        androidMain.dependencies {
            implementation(libs.ktor.client.okhttp)
        }
        if (iosEnabled) {
            iosMain.dependencies {
                implementation(libs.ktor.client.darwin)
            }
        }
    }
}

tasks.withType<KotlinCompilationTask<*>>().configureEach { dependsOn(tasks.named<GenerateTask>("openApiGenerate")) }
tasks.matching { it.name.endsWith("SourcesJar") }.configureEach { dependsOn(tasks.named<GenerateTask>("openApiGenerate")) }

tasks.named<Test>("jvmTest") {
    // ContractTest reads the same contract the client is generated from.
    systemProperty("openapi.spec", openapiSpec.absolutePath)
    inputs.file(openapiSpec).withPropertyName("openapiSpec")
}

// Drift check (the web's `pnpm gen:api && git diff --exit-code`): the client is not committed,
// so drift shows as the app or its API tests no longer compiling or passing against a client
// generated afresh from openapi.json (or -PopenapiSpec=<path>).
val cleanApiClient by tasks.registering(Delete::class) {
    delete(generatedApiDir)
}
tasks.named("openApiGenerate") { mustRunAfter(cleanApiClient) }
tasks.register("checkApiClient") {
    group = "verification"
    description = "Regenerates the API client from openapi.json, then compiles and tests against it."
    dependsOn(cleanApiClient, "openApiGenerate", "compileKotlinJvm", "compileAndroidMain", "jvmTest")
}
