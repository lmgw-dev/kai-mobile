import com.android.build.api.dsl.ApplicationExtension
import javax.inject.Inject
import org.gradle.process.ExecOperations

// WP0's device spike (Kai mobile design §17): trials run by hand on a foldable test phone and a
// LineageOS device. A separate app (dev.rootshell.kai.spike), so it never touches the real app's
// assistant role, Keystore aliases or pairing. Configured only with -Pkai.spike=true
// (android/settings.gradle.kts); built by `just spike-apk`.
//
// One variant: release, not debuggable (a debuggable build runs slower and would skew every
// timing), not minified (R8 is no part of what is measured, and it must not strip what JNA and
// libwebrtc reach by reflection), signed with the SDK's debug key so it installs as is.
// arm64-v8a only: both test devices are arm64.

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The spike's library (just spike-core): libkai_wp0_spike.so, the core and the spike-only FFI
// (spikes/wp0/ffi) in one, with both crates' Kotlin bindings.
abstract class SpikeCoreTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations

    @get:Internal abstract val repoRoot: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:OutputDirectory abstract val jniLibs: DirectoryProperty
    @get:OutputDirectory abstract val bindings: DirectoryProperty

    @TaskAction
    fun build() {
        exec.exec {
            workingDir(repoRoot.get().asFile)
            commandLine("just", "spike-core", jniLibs.get().asFile.path, bindings.get().asFile.path)
        }
    }
}

val kaiRepoRoot = rootProject.layout.projectDirectory.dir("..")
val appSources = rootProject.layout.projectDirectory.dir("app/src/main/kotlin/dev/rootshell/kai")

extensions.configure<ApplicationExtension> {
    namespace = "dev.rootshell.kai.spike"
    compileSdk = 37
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "dev.rootshell.kai.spike"
        minSdk = 31
        targetSdk = 37
        versionCode = 1
        versionName = "wp0"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    testBuildType = "release"

    buildTypes {
        release {
            isMinifyEnabled = false
            isDebuggable = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    buildFeatures { compose = true }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    packaging {
        // Stored, 16 KB aligned libraries (design §2.1), as in the app: (e) checks them on the phone.
        jniLibs { useLegacyPackaging = false }
    }

    lint {
        // A spike: lint reports, never blocks (just gradle-verification runs it with the spike).
        abortOnError = false
        checkReleaseBuilds = false
    }
}

kotlin { jvmToolchain(21) }

androidComponents {
    beforeVariants(selector().withBuildType("debug")) { it.enable = false }
    onVariants { variant ->
        val suffix = variant.name.replaceFirstChar { it.uppercase() }
        val core = tasks.register<SpikeCoreTask>("buildSpikeCore$suffix") {
            group = "build"
            description = "Builds libkai_wp0_spike.so (just spike-core) and its Kotlin bindings"
            repoRoot.set(kaiRepoRoot)
            jniLibs.set(layout.buildDirectory.dir("core/${variant.name}/jniLibs"))
            bindings.set(layout.buildDirectory.dir("core/${variant.name}/kotlin"))
            sources.from(
                fileTree(kaiRepoRoot.dir("core")) {
                    include("**/*.rs", "**/Cargo.toml", "**/Cargo.lock", "**/uniffi.toml", ".cargo/config.toml")
                    exclude("target/**")
                },
                fileTree(kaiRepoRoot.dir("spikes/wp0/ffi")) { include("**/*.rs", "**/*.toml") },
                kaiRepoRoot.file("LMGW_PIN"),
                kaiRepoRoot.file("justfile"),
                fileTree(kaiRepoRoot.dir(".stage/lmgw")),
            )
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(core, SpikeCoreTask::jniLibs)
        variant.sources.kotlin?.addGeneratedSourceDirectory(core, SpikeCoreTask::bindings)
        // The scaffold's theme and core facade, compiled here as they are (the theme is the app's;
        // the facade runs the real app's first call into the core).
        variant.sources.kotlin?.addStaticSourceDirectory(appSources.dir("ui/theme").asFile.path)
        variant.sources.kotlin?.addStaticSourceDirectory(appSources.dir("core").asFile.path)
        tasks.matching { it.name == "pre${suffix}Build" }.configureEach { dependsOn(core) }
    }
}

dependencies {
    val bom = platform(libs.androidx.compose.bom)
    implementation(bom)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material3.adaptive)

    implementation("${libs.jna.get()}@aar")
    implementation(libs.kotlinx.coroutines.android)
    // LiveKit's libwebrtc build, used standalone through org.webrtc (design §7, WP0 (a) and (o)).
    implementation(libs.webrtc)

    testImplementation(libs.junit)
}
