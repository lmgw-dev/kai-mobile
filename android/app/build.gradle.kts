import com.android.build.api.dsl.ApplicationExtension
import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// ---------------------------------------------------------------------------------------------
// The core (design §2.2): `just core-android` builds libkai_mobile_core.so with cargo-ndk and
// generates the Kotlin bindings. Each variant has its own task and its own output directories, so
// a debug and a release build in one Gradle run cannot overwrite each other: debug carries both
// ABIs, release arm64-v8a only. The outputs are registered as the variant's jniLibs and Kotlin
// sources (so everything that reads them waits for the task) and the variant's preBuild depends on
// the task as well.
// ---------------------------------------------------------------------------------------------
abstract class CoreAndroidTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations

    @get:Input abstract val profile: Property<String>
    @get:Input abstract val abis: ListProperty<String>
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
            commandLine(
                "just", "core-android", profile.get(),
                jniLibs.get().asFile.path, bindings.get().asFile.path, abis.get().joinToString(" "),
            )
        }
    }
}

val kaiRepoRoot = rootProject.layout.projectDirectory.dir("..")

// ---- versions (design §2.2, README.md "Versions"). Three kinds of build:
//  - the private CI's (KAI_VERSION_CODE = the pipeline IID): versionName <kai.version>+<IID>,
//    versionCode the IID, which only grows within that project;
//  - a local build: <kai.version>+local, versionCode 1 (AGP refuses 0), never mistaken for a CI build;
//  - a public release (KAI_PUBLIC_RELEASE=1, set by .github/workflows/release.yml): versionName is
//    the plain <kai.version>, versionCode = 100_000_000 + major * 1_000_000 + minor * 1_000 + patch.
//    That rises with every release, tops out at 1_099_999_999 (Android allows 2_100_000_000), and
//    stays above any pipeline IID, so a phone moves from a private build to a public release and
//    never back (a downgrade is refused by Android).
val kaiVersion = providers.gradleProperty("kai.version").get()
val publicRelease = providers.environmentVariable("KAI_PUBLIC_RELEASE").orNull == "1"
val ciVersionCode = providers.environmentVariable("KAI_VERSION_CODE").map { it.toInt() }.orNull
val publicVersionCodeBase = 100_000_000

fun publicVersionCode(version: String): Int {
    val fields = Regex("""(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)""").matchEntire(version)
        ?.groupValues?.drop(1)?.map { it.toIntOrNull() ?: Int.MAX_VALUE }
        ?: throw GradleException("kai.version \"$version\" is not X.Y.Z: a public release needs one")
    if (fields.any { it > 999 }) {
        throw GradleException("kai.version $version has a field above 999: the public versionCode has three digits for each")
    }
    return publicVersionCodeBase + fields[0] * 1_000_000 + fields[1] * 1_000 + fields[2]
}

if (publicRelease && ciVersionCode != null) {
    throw GradleException("KAI_PUBLIC_RELEASE=1 and KAI_VERSION_CODE are both set: a build is a public release or a CI build")
}
if (ciVersionCode != null && ciVersionCode !in 1 until publicVersionCodeBase) {
    throw GradleException("KAI_VERSION_CODE $ciVersionCode is outside 1..${publicVersionCodeBase - 1}: higher codes belong to public releases")
}
val kaiVersionCode = if (publicRelease) publicVersionCode(kaiVersion) else ciVersionCode ?: 1
val kaiVersionName = if (publicRelease) kaiVersion else "$kaiVersion+${ciVersionCode ?: "local"}"

// ---- release signing: the keystore is a protected CI file variable, never in the repository (design
// §15.1). KAI_KEYSTORE_FILE is the file's path; without it the release APK comes out unsigned, which
// the private CI forbids with KAI_REQUIRE_SIGNING=1. The public release does not sign here: Gradle
// plugins and the core's build scripts are third-party code, so GitHub's release.yml builds the
// unsigned APK with no secret in reach and signs it afterwards in a container of its own
// (scripts/github-sign.sh).
val keystoreFile = providers.environmentVariable("KAI_KEYSTORE_FILE").orNull
gradle.taskGraph.whenReady {
    val releasing = allTasks.any { it.name == "assembleRelease" || it.name == "bundleRelease" }
    if (releasing && keystoreFile == null) {
        val message = "KAI_KEYSTORE_FILE is not set: the release APK is built UNSIGNED (app-release-unsigned.apk)."
        if (System.getenv("KAI_REQUIRE_SIGNING") == "1") {
            throw GradleException("$message KAI_REQUIRE_SIGNING=1 forbids that; set the signing variables of README.md.")
        }
        logger.warn("WARNING: $message")
    }
}

extensions.configure<ApplicationExtension> {
    namespace = "dev.rootshell.kai"
    // M15: the newest stable API level at WP1 is 37 (Android 17); minSdk 31 for setCommunicationDevice.
    compileSdk = 37
    // The NDK of ci/Containerfile: AGP strips the packaged libraries with its tools.
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "dev.rootshell.kai"
        minSdk = 31
        targetSdk = 37
        versionCode = kaiVersionCode
        versionName = kaiVersionName
    }

    signingConfigs {
        if (keystoreFile != null) {
            create("release") {
                storeFile = file(keystoreFile)
                storePassword = System.getenv("KAI_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KAI_KEY_ALIAS")
                keyPassword = System.getenv("KAI_KEY_PASSWORD") ?: System.getenv("KAI_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        debug {
            // Installs beside a release (design M1); the SDK's debug key signs it.
            applicationIdSuffix = ".dev"
            versionNameSuffix = "-debug"
            // The core's two ABIs; the filter also keeps JNA's AAR (which carries every ABI there
            // ever was, mips included) to the same two.
            ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
            // The release carries arm64-v8a only (every phone it targets is arm64);
            // debug carries x86_64 as well for emulators.
            ndk { abiFilters += listOf("arm64-v8a") }
        }
    }

    buildFeatures { compose = true }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    packaging {
        // The 16 KB alignment of stored libraries (design §2.1): never compressed in the APK.
        jniLibs { useLegacyPackaging = false }
    }

    lint {
        abortOnError = true
        warningsAsErrors = false
        checkReleaseBuilds = true
        // The release carries arm64-v8a only on purpose (arm64 phones are the target).
        disable += "ChromeOsAbiSupport"
    }

    testOptions { unitTests.isReturnDefaultValues = false }
}

kotlin { jvmToolchain(21) }

androidComponents {
    onVariants { variant ->
        val suffix = variant.name.replaceFirstChar { it.uppercase() }
        val release = variant.buildType == "release"
        val core = tasks.register<CoreAndroidTask>("buildCore$suffix") {
            group = "build"
            description = "Builds the Rust core for ${variant.name} (just core-android) and generates its Kotlin bindings"
            profile.set(if (release) "release" else "debug")
            abis.set(if (release) listOf("arm64-v8a") else listOf("arm64-v8a", "x86_64"))
            repoRoot.set(kaiRepoRoot)
            jniLibs.set(layout.buildDirectory.dir("core/${variant.name}/jniLibs"))
            bindings.set(layout.buildDirectory.dir("core/${variant.name}/kotlin"))
            sources.from(
                fileTree(kaiRepoRoot.dir("core")) {
                    include("**/*.rs", "**/Cargo.toml", "**/Cargo.lock", "**/uniffi.toml", ".cargo/config.toml")
                    exclude("target/**")
                },
                kaiRepoRoot.file("LMGW_PIN"),
                kaiRepoRoot.file("justfile"),
                fileTree(kaiRepoRoot.dir(".stage/lmgw")),
            )
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(core, CoreAndroidTask::jniLibs)
        variant.sources.kotlin?.addGeneratedSourceDirectory(core, CoreAndroidTask::bindings)
        tasks.matching { it.name == "pre${suffix}Build" }.configureEach { dependsOn(core) }
    }
}

// Lint reads every variant's source directories from disk (its project model lists them all), but
// only the release lint tasks depend on buildCoreRelease, and Gradle runs tasks in parallel: a debug
// lint task (lintAnalyzeDebug …) could read build/core/release/kotlin while buildCoreRelease's
// `rm -rf` and rewrite (just core-android) emptied it, so `lint assembleRelease` in one invocation
// failed now and then. Every lint task and lint model task therefore runs after every core task.
// mustRunAfter only orders: it pulls no core build into a lint-only run.
tasks.matching { it.name.startsWith("lint") || (it.name.startsWith("generate") && it.name.contains("Lint")) }
    .configureEach { mustRunAfter(tasks.withType<CoreAndroidTask>()) }

dependencies {
    val bom = platform(libs.androidx.compose.bom)
    implementation(bom)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons)
    implementation(libs.androidx.compose.material3.adaptive)
    implementation(libs.androidx.compose.ui.tooling.preview)
    debugImplementation(libs.androidx.compose.ui.tooling)

    // The pairing link's QR code (design §4.1, M6): CameraX and ZXing's core, no Play services.
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
    implementation(libs.zxing.core)

    // Voice (design §7): LiveKit's libwebrtc build, standalone through org.webrtc, as WP0's spike.
    implementation(libs.webrtc)

    // UniFFI's Kotlin bindings: JNA (the AAR carries libjnidispatch per ABI) and coroutines.
    implementation("${libs.jna.get()}@aar")
    implementation(libs.kotlinx.coroutines.android)

    testImplementation(libs.junit)
}
