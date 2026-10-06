plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.starinin.raytrace"
    compileSdk = 35
    buildToolsVersion = "35.0.0"
    defaultConfig {
        applicationId = "dev.starinin.raytrace"
        minSdk = 28
        targetSdk = 34
        versionCode = 1
        versionName = "1.0.0"
        ndk { abiFilters += "arm64-v8a" }
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    packaging { jniLibs { useLegacyPackaging = true } }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}

val buildNative by tasks.registering(Exec::class) {
    workingDir = rootDir
    commandLine("bash", "tools/build-native.sh")
    inputs.dir(rootProject.file("rust/src"))
    inputs.file(rootProject.file("rust/Cargo.toml"))
    outputs.dir(file("src/main/jniLibs"))
}
tasks.matching { it.name == "preBuild" }.configureEach { dependsOn(buildNative) }
