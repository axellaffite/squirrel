import org.jetbrains.kotlin.gradle.tasks.KotlinCompilationTask
import org.openapitools.generator.gradle.plugin.tasks.GenerateTask

plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.android.kmp.library)
    alias(libs.plugins.openapi.generator)
    alias(libs.plugins.sqldelight)
}

val iosEnabled = providers.gradleProperty("squirrel.ios").map(String::toBoolean).getOrElse(false)

// The committed contract; -PopenapiSpec=<path> points the drift check at another copy.
val openapiSpec: File = providers.gradleProperty("openapiSpec")
    .map { file(it) }
    .getOrElse(rootProject.file("../openapi.json"))
val generatedApiDir = layout.buildDirectory.dir("generated/openapi")

openApiValidate {
    inputSpec.set(openapiSpec.path)
}

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
    validateSpec.set(true)
}

// Models that carry passwords or tokens print as "<redacted>": a stray log line or crash report
// must never contain a secret. Patched into the generated sources (no template fork needed).
tasks.named("openApiGenerate") {
    val models = generatedApiDir.map { it.dir("src/commonMain/kotlin/app/squirrel/api/models") }
    doLast {
        val secretModels = listOf(
            "DeviceLoginRequest", "RefreshRequest", "TokenPair", "LoginRequest", "SetupRequest", "AccountRequest",
        )
        secretModels.forEach { name ->
            val file = models.get().file("$name.kt").asFile
            val text = file.readText()
            check("data class $name (" in text) { "$name is no longer a generated data class" }
            val end = text.lastIndexOf('}')
            file.writeText(text.substring(0, end) + "    override fun toString(): String = \"$name(<redacted>)\"\n" + text.substring(end))
        }
    }
}

kotlin {
    jvmToolchain(libs.versions.jvm.toolchain.get().toInt())

    compilerOptions { freeCompilerArgs.add("-Xexpect-actual-classes") }

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
            kotlin.srcDir(files(generatedApiDir.map { it.dir("src/commonMain/kotlin") }).builtBy("openApiGenerate"))
            dependencies {
                api(libs.coroutines.core)
                api(libs.ktor.client.core)
                api(libs.datetime)
                implementation(libs.ktor.client.auth)
                implementation(libs.ktor.client.content.negotiation)
                implementation(libs.ktor.serialization.json)
                implementation(libs.serialization.json)
                implementation(libs.sqldelight.coroutines)
            }
        }
        commonTest.dependencies {
            implementation(libs.kotlin.test)
            implementation(libs.coroutines.test)
            implementation(libs.ktor.client.mock)
            implementation(libs.turbine)
        }
        jvmMain.dependencies {
            implementation(libs.sqldelight.jvm)
        }
        androidMain.dependencies {
            implementation(libs.ktor.client.okhttp)
            implementation(libs.sqldelight.android)
        }
        if (iosEnabled) {
            iosMain.dependencies {
                implementation(libs.ktor.client.darwin)
                implementation(libs.sqldelight.native)
            }
        }
    }
}

sqldelight {
    databases {
        create("SquirrelDatabase") {
            packageName.set("app.squirrel.shared.db")
            // Default dialect (SQLite 3.18): Android 10, the minimum, ships 3.22 (no UPSERT).
            // Every schema version is snapshotted (N.db) and each N.sqm is verified against them.
            // After a schema change, run :shared:generateCommonMainSquirrelDatabaseSchema on its own.
            schemaOutputDirectory.set(file("src/commonMain/sqldelight/databases"))
            verifyMigrations.set(true)
        }
    }
}

tasks.withType<KotlinCompilationTask<*>>().configureEach { dependsOn(tasks.named<GenerateTask>("openApiGenerate")) }
// AGP also reads the generated source root (e.g. its baselineProfiles folder).
tasks.matching { it.name.endsWith("SourcesJar") || it.name.contains("ArtProfile") }.configureEach { dependsOn(tasks.named<GenerateTask>("openApiGenerate")) }

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
    dependsOn(cleanApiClient, "openApiValidate", "openApiGenerate", "compileKotlinJvm", "compileAndroidMain", "jvmTest")
}
