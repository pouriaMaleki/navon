import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "me.fiksu.esp32map.companion"
    compileSdk = libs.versions.androidCompileSdk.get().toInt()

    defaultConfig {
        applicationId = "me.fiksu.esp32map.companion"
        minSdk = libs.versions.androidMinSdk.get().toInt()
        targetSdk = libs.versions.androidTargetSdk.get().toInt()
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
        manifestPlaceholders += mapOf(
            "googleMapsApiKey" to (System.getenv("GOOGLE_MAPS_API_KEY") ?: ""),
        )
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// Pin transitive dependencies that arrive via Robolectric and other test
// libs to versions Dependabot considers safe. Without these forces,
// older Bouncy Castle (< 1.84) and Guava (< 32.0.0-android) flow in
// from upstream artifacts and trip the GitHub security alerts even
// though our app code never uses them directly.
//
// See dependabot alerts #29 (bcpkix), #30/#35 (bcprov), #33/#34 (guava)
// on https://github.com/pouriaMaleki/esp32-map/security/dependabot
configurations.all {
    resolutionStrategy {
        force("org.bouncycastle:bcprov-jdk18on:1.84")
        force("org.bouncycastle:bcpkix-jdk18on:1.84")
        force("org.bouncycastle:bcutil-jdk18on:1.84")
        force("com.google.guava:guava:32.0.1-android")
    }
}

dependencies {
    val composeBom = platform(libs.androidx.compose.bom)

    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.google.play.services.maps)
    implementation(libs.google.play.services.location)
    implementation(libs.google.maps.compose)
    implementation(libs.google.android.material)
    implementation(libs.google.gson)
    implementation(libs.kotlinx.coroutines.android)

    // CameraX + ML Kit for the pairing-flow QR scan. The pairing flow
    // is the only screen that opens the camera; CameraX is the
    // current-best AndroidX abstraction over `Camera2`, and ML Kit's
    // on-device barcode scanner avoids any network round-trip for the
    // QR decode (the secret never leaves the device).
    implementation("androidx.camera:camera-core:1.3.4")
    implementation("androidx.camera:camera-camera2:1.3.4")
    implementation("androidx.camera:camera-lifecycle:1.3.4")
    implementation("androidx.camera:camera-view:1.3.4")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")

    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)

    testImplementation(libs.junit4)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.turbine)
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.ext.junit)

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
}
