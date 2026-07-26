import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("dev.flutter.flutter-gradle-plugin")
}

val signingProperties = Properties()
val signingPropertiesFile = rootProject.file("signing.properties")

if (!signingPropertiesFile.isFile) {
    throw GradleException(
        "Missing Android release signing configuration. Create android/signing.properties from the documented template before building a release APK.",
    )
}
signingPropertiesFile.inputStream().use(signingProperties::load)

android {
    namespace = "dev.prayag.apptester.companion"
    compileSdk = flutter.compileSdkVersion

    defaultConfig {
        applicationId = "dev.prayag.apptester.companion"
        minSdk = 26
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.create("release") {
                storeFile = rootProject.file(signingProperties.getProperty("storeFile"))
                storePassword = signingProperties.getProperty("storePassword")
                keyAlias = signingProperties.getProperty("keyAlias")
                keyPassword = signingProperties.getProperty("keyPassword")
            }
        }
    }
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8)
}

flutter { source = "../.." }
