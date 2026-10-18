import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application") version "8.13.2"
    id("org.jetbrains.kotlin.android") version "2.2.21"
}

val repository = rootDir.resolve("../../..").canonicalFile
layout.buildDirectory.set(repository.resolve("target/android-tests/build"))

android {
    namespace = "lol.cutout.ffitest"
    compileSdk = providers.environmentVariable("CUTOUT_ANDROID_API").get().toInt()
    buildToolsVersion = providers.environmentVariable("CUTOUT_ANDROID_BUILD_TOOLS").get()
    ndkPath = providers.environmentVariable("ANDROID_NDK_ROOT").get()
    defaultConfig {
        applicationId = "lol.cutout.ffitest"
        minSdk = 26
        targetSdk = compileSdk
        versionCode = 1
        versionName = "1.0"
        ndk { abiFilters += "arm64-v8a" }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    sourceSets["main"].apply {
        java.srcDirs("../shared", repository.resolve("target/uniffi-smoke/kotlin"),
            repository.resolve("target/uniffi-smoke/replay"))
        jniLibs.srcDir(repository.resolve("target/android-tests/jniLibs"))
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    packaging { jniLibs.useLegacyPackaging = true }
}
kotlin { compilerOptions.jvmTarget.set(JvmTarget.JVM_17) }

dependencies {
    implementation("net.java.dev.jna:jna:${providers.environmentVariable("CUTOUT_JNA_VERSION").get()}@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("junit:junit:4.13.2")
}
