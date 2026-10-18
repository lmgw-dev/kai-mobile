pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
    }
}

rootProject.name = "kai-mobile"
include(":app")

// WP0's device spike (spikes/wp0/app, applicationId dev.rootshell.kai.spike): only with
// -Pkai.spike=true (just spike-apk), so the app's builds, checks and CI never configure it.
if (providers.gradleProperty("kai.spike").orNull == "true") {
    include(":spike")
    project(":spike").projectDir = file("../spikes/wp0/app")
}
