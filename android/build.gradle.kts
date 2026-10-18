// The Gradle project is a plain Android project that Android tooling opens without knowing Rust
// exists (design M1); the core's build is app/build.gradle.kts's one task.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
