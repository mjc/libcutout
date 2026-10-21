import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.ValueSource
import org.gradle.api.provider.ValueSourceParameters

buildscript {
    repositories {
        mavenCentral()
    }
    dependencies {
        // AGP owns Kotlin application; this selects its compiler implementation.
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.21")
    }
}

plugins {
    id("com.android.application") version "9.4.1"
}

val nativeAbi = providers.gradleProperty("cutout.android.abi").orElse("arm64-v8a").get()
require(nativeAbi in setOf("arm64-v8a", "x86_64")) { "Unsupported Cutout JNI ABI: $nativeAbi" }

val repository = rootDir.parentFile
val androidApi = providers.environmentVariable("CUTOUT_ANDROID_API").get().toInt()
val androidMinorApi = providers.environmentVariable("CUTOUT_ANDROID_API_MINOR").get().toInt()

abstract class RustlsPlatformVerifierVersion : ValueSource<String, RustlsPlatformVerifierVersion.Parameters> {
    interface Parameters : ValueSourceParameters {
        val lockFile: RegularFileProperty
    }

    override fun obtain(): String {
        val block = parameters.lockFile.get().asFile.readText()
            .split("[[package]]")
            .single { packageBlock ->
                packageBlock.lineSequence().any {
                    it.trim() == "name = \"rustls-platform-verifier-android\""
                }
            }
        return block.lineSequence().first { it.trimStart().startsWith("version = ") }
            .substringAfter('"').substringBefore('"')
    }
}

val rustlsPlatformVerifierVersion = providers.of(RustlsPlatformVerifierVersion::class.java) {
    parameters.lockFile.set(layout.projectDirectory.file("../Cargo.lock"))
}
layout.buildDirectory.set(repository.resolve("target/android-tests/build"))

android {
    namespace = "lol.cutout"
    compileSdk {
        version = release(androidApi) {
            minorApiLevel = androidMinorApi
        }
    }
    buildToolsVersion = providers.environmentVariable("CUTOUT_ANDROID_BUILD_TOOLS").get()
    ndkPath = providers.environmentVariable("ANDROID_NDK_ROOT").get()
    defaultConfig {
        applicationId = "lol.cutout"
        minSdk = providers.environmentVariable("CUTOUT_ANDROID_MIN_API").get().toInt()
        targetSdk = androidApi
        versionCode = 1
        versionName = "1.0"
        ndk { abiFilters += nativeAbi }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    sourceSets["main"].apply {
        kotlin.directories += repository.resolve("target/uniffi-smoke/kotlin").path
        jniLibs.directories += repository.resolve("target/android-tests/jniLibs").path
        res.directories += repository.resolve("target/android-tests/resources").path
    }
    sourceSets["androidTest"].apply {
        kotlin.directories += listOf(
            repository.resolve("tests/mobile-ffi/shared").path,
            repository.resolve("target/uniffi-smoke/replay").path,
        )
    }
    sourceSets["test"].apply {
        // Compile the same behavioral tests for instrumentation and Robolectric.
        kotlin.directories += listOf(
            repository.resolve("android/src/androidTest/java").path,
            repository.resolve("tests/mobile-ffi/shared").path,
            repository.resolve("target/uniffi-smoke/replay").path,
        )
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    packaging { jniLibs.useLegacyPackaging = true }
    testOptions { unitTests.isIncludeAndroidResources = true }
}
kotlin { compilerOptions.jvmTarget.set(JvmTarget.JVM_17) }

tasks.withType<Test>().configureEach {
    maxParallelForks = 1
    maxHeapSize = "768m"
    jvmArgs("-XX:MaxMetaspaceSize=256m")
    filter { includeTestsMatching("lol.cutout.local.*") }
    systemProperty("cutout.ui.artifacts", providers.gradleProperty("cutout.ui.artifacts")
        .orElse(layout.buildDirectory.dir("ui-artifacts").map { it.asFile.absolutePath }).get())
    providers.gradleProperty("cutout.native.library.dir").orNull?.let { libraryDir ->
        systemProperty("jna.library.path", libraryDir)
        systemProperty("java.library.path", libraryDir)
    }
}

configurations.configureEach {
    resolutionStrategy.eachDependency {
        if (requested.group == "org.rustls" && requested.name == "rustls-platform-verifier") {
            useVersion(rustlsPlatformVerifierVersion.get())
            because("Android verifier support must match the Rust Cargo.lock component")
        }
    }
    if (name.endsWith("UnitTestRuntimeClasspath")) {
        exclude(group = "net.java.dev.jna", module = "jna")
    }
}

dependencies {
    implementation("androidx.core:core:1.19.1")
    implementation("org.rustls:rustls-platform-verifier")
    implementation("org.maplibre.gl:android-sdk-opengl:13.7.1")
    implementation("net.java.dev.jna:jna:${providers.environmentVariable("CUTOUT_JNA_VERSION").get()}@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("junit:junit:4.13.2")
    testImplementation("junit:junit:4.13.2")
    testImplementation("androidx.test:core:1.7.0")
    testImplementation("org.robolectric:robolectric:4.17")
    testImplementation(files(providers.environmentVariable("JNA_JAR").get()))
}
