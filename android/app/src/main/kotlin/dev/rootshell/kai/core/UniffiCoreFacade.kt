package dev.rootshell.kai.core

import android.content.Context
import android.os.Build
import android.provider.Settings as AndroidSettings
import dev.rootshell.kai.core.ffi.AppDirs
import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.Core
import dev.rootshell.kai.core.ffi.Decided
import dev.rootshell.kai.core.ffi.EventSink
import dev.rootshell.kai.core.ffi.HeadsetPress
import dev.rootshell.kai.core.ffi.NetworkFacts
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.PairingPreview
import dev.rootshell.kai.core.ffi.PairingView
import dev.rootshell.kai.core.ffi.PlatformFacts
import dev.rootshell.kai.core.ffi.PlayoutStats
import dev.rootshell.kai.core.ffi.SearchIn
import dev.rootshell.kai.core.ffi.SearchPageView
import dev.rootshell.kai.core.ffi.Settings
import dev.rootshell.kai.core.ffi.TransportState
import dev.rootshell.kai.core.ffi.Update
import dev.rootshell.kai.core.ffi.Voice

/** The real core, over the UniFFI bindings, with the Android Keystore behind its callback. */
class UniffiCoreFacade private constructor(private val core: Core) : CoreFacade {
    override fun version(): String = core.version()

    override fun lmgwPin(): LmgwPin = core.lmgwPin().let { LmgwPin(it.commit, it.tag, it.tree) }

    override fun build(): CoreBuild = core.about().let { CoreBuild(it.profile, it.target) }

    override fun colours(): Colours = when (core.settings().appearance.colours) {
        dev.rootshell.kai.core.ffi.Colours.KAI -> Colours.Kai
        dev.rootshell.kai.core.ffi.Colours.SYSTEM -> Colours.System
    }

    override fun settingsNotice(): String? = core.settingsNotice()
    override fun settings(): Settings = core.settings()
    override suspend fun setSettings(settings: Settings) = core.setSettings(settings)
    override fun paired(): Boolean = core.paired()
    override suspend fun pairingPreview(text: String): PairingPreview = core.pairingPreview(text)
    override suspend fun pair(text: String): PairingView = core.pair(text)
    override suspend fun forget() = core.forget()
    override fun retry() = core.retry()
    override fun setNetwork(net: NetworkFacts) = core.setNetwork(net)
    override fun setForeground(visible: Boolean) = core.setForeground(visible)
    override fun setOverlay(showingCards: Boolean) = core.setOverlay(showingCards)
    override fun wake() = core.wake()
    override suspend fun nextUpdate(): Update = core.nextUpdate()
    override suspend fun sendText(text: String) = core.sendText(text)
    override suspend fun newConversation() = core.newConversation()
    override suspend fun decide(approvalRequestId: String, approve: Boolean): Decided =
        core.decide(approvalRequestId, approve)
    override suspend fun dismissApproval(approvalRequestId: String) = core.dismissApproval(approvalRequestId)
    override fun notificationGone(id: NotificationId) = core.notificationGone(id)
    override fun browser(showing: Boolean) = core.browser(showing)
    override fun browserArchived(showing: Boolean) = core.browserArchived(showing)
    override suspend fun search(query: String, scope: SearchIn, folderId: Long?, offset: Long?): SearchPageView =
        core.search(query, scope, folderId, offset)
    override fun openThread(threadId: Long) = core.openThread(threadId)
    override fun closeThread() = core.closeThread()
    override suspend fun sendInThread(threadId: Long, text: String) = core.sendInThread(threadId, text)
    override suspend fun newThread(folderId: Long): Long = core.newThread(folderId)
    override suspend fun pinThread(threadId: Long, pinned: Boolean) = core.pinThread(threadId, pinned)
    override suspend fun archiveThread(threadId: Long, archived: Boolean) = core.archiveThread(threadId, archived)
    override suspend fun moveThread(threadId: Long, folderId: Long?) = core.moveThread(threadId, folderId)
    override suspend fun renameThread(threadId: Long, title: String) = core.renameThread(threadId, title)
    override suspend fun deleteThread(threadId: Long) = core.deleteThread(threadId)

    override val voice: VoiceFacade = UniffiVoice(core.voice())

    /** The core's `Voice`, as it is. */
    private class UniffiVoice(private val v: Voice) : VoiceFacade {
        override fun prepare() = v.prepare()
        override suspend fun start(start: CallStart, sink: EventSink): ULong = v.start(start, sink)
        override suspend fun switchTo(start: CallStart): Boolean = v.switchTo(start)
        override suspend fun stay(call: ULong) = v.stay(call)
        override suspend fun offer(call: ULong, sdp: String): String = v.offer(call, sdp)
        override fun transport(call: ULong, state: TransportState, detail: String) = v.transport(call, state, detail)
        override fun channelOpen(call: ULong) = v.channelOpen(call)
        override fun channelMessage(call: ULong, text: String) = v.channelMessage(call, text)
        override fun channelClosed(call: ULong, why: String) = v.channelClosed(call, why)
        override fun failed(call: ULong, words: String) = v.failed(call, words)
        override fun takeOver() = v.takeOver()
        override fun press() = v.press()
        override fun release() = v.release()
        override fun tap() = v.tap()
        override fun stop() = v.stop()
        override fun setMuted(muted: Boolean) = v.setMuted(muted)
        override fun setPushToTalk(on: Boolean) = v.setPushToTalk(on)
        override fun capturing(call: ULong) = v.capturing(call)
        override fun focus(call: ULong, focus: AudioFocus) = v.focus(call, focus)
        override fun headset(press: HeadsetPress) = v.headset(press)
        override fun stats(call: ULong, request: ULong, stats: PlayoutStats) = v.stats(call, request, stats)
        override fun end() = v.end()
        override fun dismissed(call: ULong) = v.dismissed(call)
    }

    companion object {
        /**
         * The core over the app's storage. `notifiedApprovals` and `notifiedReplies` are the system's
         * active notifications at start (design §5.4: the notified ids outlive the process).
         */
        fun create(
            context: Context,
            notifiedApprovals: List<String> = emptyList(),
            notifiedReplies: List<Long> = emptyList(),
        ): UniffiCoreFacade {
            val version = runCatching {
                context.packageManager.getPackageInfo(context.packageName, 0).versionName
            }.getOrNull().orEmpty()
            return UniffiCoreFacade(
                Core(
                    AppDirs(dataDir = context.filesDir.path, cacheDir = context.cacheDir.path),
                    PlatformFacts(
                        os = "Android ${Build.VERSION.RELEASE}",
                        appVersion = version,
                        appId = context.packageName,
                        deviceName = deviceName(context),
                        notifiedApprovals = notifiedApprovals,
                        notifiedReplies = notifiedReplies,
                    ),
                    AndroidKeystore(),
                ),
            )
        }

        /**
         * The name the user gave the phone (Settings → About phone → Device name), else the model name; null when
         * both are blank. Enrolment sends it as the device's own claim.
         */
        private fun deviceName(context: Context): String? =
            AndroidSettings.Global.getString(context.contentResolver, AndroidSettings.Global.DEVICE_NAME)
                ?.takeIf { it.isNotBlank() }
                ?: Build.MODEL?.takeIf { it.isNotBlank() }
    }
}
