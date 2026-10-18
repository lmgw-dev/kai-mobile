package dev.rootshell.kai.spike.probe

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.app.role.RoleManager
import android.content.Context
import android.os.Build
import dev.rootshell.kai.core.UniffiCoreFacade
import dev.rootshell.kai.spike.Device
import dev.rootshell.kai.spike.R
import dev.rootshell.kai.spike.SpikeApp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.assist.SpikeInteractionService
import dev.rootshell.kai.spike.granted
import dev.rootshell.kai.spike.assist.SpikeSession
import dev.rootshell.kai.spike.ffi.spikeInfo
import dev.rootshell.kai.spike.msSince
import dev.rootshell.kai.spike.r2
import dev.rootshell.kai.spike.webrtc.WebRtc
import java.security.KeyStore
import java.security.Signature
import java.util.UUID

/**
 * (o): a device without Google's services (e.g. LineageOS) (design M17, WP0 (o)), in one run: the
 * Android version (M15), the core, libwebrtc, a Keystore key that signs, the overlay and a
 * notification. Runs on any test phone.
 */
object SelfCheck {
    data class Item(val name: String, val ok: Boolean?, val ms: Double?, val detail: String) {
        fun toMap(): Map<String, Any?> = mapOf("name" to name, "ok" to ok, "ms" to ms, "detail" to detail)
        override fun toString() = "${when (ok) { true -> "pass"; false -> "FAIL"; null -> "skip" }} · $name" +
            (ms?.let { " · $it ms" } ?: "") + " · $detail"
    }

    fun run(context: Context): List<Item> {
        val items = mutableListOf<Item>()
        fun check(name: String, f: () -> Pair<Boolean?, String>) {
            val t = System.nanoTime()
            val item = try {
                val (ok, detail) = f()
                Item(name, ok, msSince(t).r2(), detail)
            } catch (e: Throwable) {
                Item(name, false, msSince(t).r2(), "${e.javaClass.simpleName}: ${e.message}")
            }
            items += item
        }
        check("Android version (M15)") {
            true to "Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT}), ${Build.MANUFACTURER} ${Build.MODEL}, " +
                "build ${Build.DISPLAY}" + Device.prop("ro.lineage.version").let { if (it.isEmpty()) "" else ", LineageOS $it" } +
                Device.prop("ro.build.version.oneui").let { if (it.isEmpty()) "" else ", One UI $it" }
        }
        check("Google's services") {
            val gms = runCatching { context.packageManager.getPackageInfo("com.google.android.gms", 0) }.isSuccess
            true to if (gms) "installed (com.google.android.gms)" else "not installed"
        }
        check("the core loads") {
            val core = UniffiCoreFacade.create(context)
            val build = core.build()
            true to "core ${core.version()} (${build.profile}, ${build.target}), lmgw ${core.lmgwPin().commit.take(7)}"
        }
        check("the spike's FFI") {
            val info = spikeInfo()
            true to "spike ${info.spikeVersion} (${info.profile}, ${info.targetArch}), ${info.tokioWorkers} tokio workers"
        }
        check("libwebrtc initialises") {
            val prep = WebRtc.prepare(context)
            if (prep.error != null) return@check false to prep.error
            val offer = WebRtc.offer()
            (offer.error == null && offer.hasAudio) to
                "initialize ${prep.initializeMs} ms, device module ${prep.admMs} ms, factory ${prep.factoryMs} ms" +
                (if (prep.alreadyPrepared) " (prepared earlier)" else "") +
                "; an audio offer in ${offer.ms} ms (${offer.sdpLines} lines, opus ${offer.opus})" +
                (offer.error?.let { "; $it" } ?: "") + "; hardware AEC ${prep.hwAecSupported}, NS ${prep.hwNsSupported}"
        }
        check("a Keystore key signs") {
            val ks = AndroidKeystore()
            val alias = "kai-spike-check-" + UUID.randomUUID()
            try {
                ks.generate(alias)
                val message = "kai spike self-check".toByteArray()
                val signature = ks.sign(alias, message)
                val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                val verified = Signature.getInstance("SHA256withECDSA").run {
                    initVerify(store.getCertificate(alias).publicKey)
                    update(message)
                    verify(signature)
                }
                verified to "P-256 key in ${AndroidKeystore.levelName(ks.lastSecurityLevel)}, signature verified $verified"
            } finally {
                ks.delete(alias)
            }
        }
        check("the overlay") {
            val roles = context.getSystemService(RoleManager::class.java)
            val held = roles.isRoleHeld(RoleManager.ROLE_ASSISTANT)
            val vis = SpikeInteractionService.instance
            when {
                !held -> null to "skipped: the spike is not the digital assistant (set it on (a)'s screen)"
                vis == null -> false to "the role is held but the assistant service is not running in this process"
                else -> {
                    val before = SpikeSession.invocations.value.size
                    vis.showFromApp("selfcheck")
                    val deadline = System.currentTimeMillis() + 4000
                    while (SpikeSession.invocations.value.size == before && System.currentTimeMillis() < deadline) Thread.sleep(50)
                    val shown = SpikeSession.invocations.value.size > before
                    shown to if (shown) SpikeSession.invocations.value.last().toString() else "no first frame within 4 s"
                }
            }
        }
        check("a notification") {
            val nm = context.getSystemService(NotificationManager::class.java)
            if (!granted(context, Manifest.permission.POST_NOTIFICATIONS)) return@check false to "POST_NOTIFICATIONS is not granted (grant it at the top of the list)"
            nm.notify(
                CHECK_ID,
                Notification.Builder(context, SpikeApp.CHANNEL_CHECK).setSmallIcon(R.drawable.ic_spike_notification)
                    .setContentTitle("Kai spike self-check").setContentText("Notifications work.").setAutoCancel(true).build(),
            )
            Thread.sleep(300)
            val posted = nm.activeNotifications.any { it.id == CHECK_ID }
            posted to "enabled ${nm.areNotificationsEnabled()}, posted $posted"
        }
        SpikeLog.event(
            "o", "selfcheck",
            mapOf("items" to items.map { it.toMap() }, "passed" to items.count { it.ok == true },
                "failed" to items.count { it.ok == false }, "skipped" to items.count { it.ok == null }) + Device.facts(context),
        )
        return items
    }

    private const val CHECK_ID = 41
}
