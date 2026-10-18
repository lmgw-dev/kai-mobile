package dev.rootshell.kai.ui.approvals

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.ApprovalView
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.ui.voice.Feel
import dev.rootshell.kai.ui.voice.rememberFeel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Decides from a card (design §11): `Core::decide` on the process's scope (a card that leaves the
 * screen mid-decision, a sheet closed or a fold, never cancels a verdict half sent), the confirm
 * haptic when the core took it, the reject haptic and the core's words on the card when it did not,
 * and the reject haptic again when lmgw's refusal of a verdict given here reaches its card (§10.5).
 * An approve while the phone is locked is refused at the tap (the locked faces show no card, so
 * this is defence in depth, as the notification's Approve asks for the unlock); a decline goes.
 */
class ApprovalDecider(private val app: KaiApplication, private val feel: Feel) {
    /** Why the last decision on a card did not go (the core's words), by approval id. */
    val errors = mutableStateMapOf<String, String>()

    /** The ids decided here whose verdicts lmgw has not answered yet (main thread). */
    private val decided = mutableSetOf<String>()

    fun decide(id: String, approve: Boolean) {
        errors.remove(id)
        if (approve && app.notifier.locked()) {
            errors[id] = app.getString(R.string.approval_unlock_first)
            feel.reject()
            return
        }
        decided += id
        app.scope.launch {
            val why = try {
                app.core.decide(id, approve)
                null
            } catch (e: CoreException) {
                e.message ?: e.javaClass.simpleName
            }
            withContext(Dispatchers.Main) {
                if (why == null) {
                    feel.confirm()
                } else {
                    decided -= id
                    errors[id] = why
                    feel.reject()
                }
            }
        }
    }

    /** Card `id` shows `note` now: lmgw's refusal of a verdict given here is felt once. */
    fun noted(id: String, note: String?) {
        if (note != null && decided.remove(id)) feel.reject()
    }
}

@Composable
fun rememberApprovalDecider(app: KaiApplication): ApprovalDecider {
    val feel = rememberFeel(app)
    return remember(app, feel) { ApprovalDecider(app, feel) }
}

/**
 * A call waiting for a decision (design §11): the tool, its arguments in compact lines (kai K22's
 * form), the thread, then Approve and Decline, or what became of the verdict (held for the reply's
 * other calls, or sent to lmgw); lmgw's words when it did not take the last one, and that saying or
 * typing something else declines it. On a voice surface ([voice]) the arguments scroll within a
 * bounded box (never cut) and the card says its place among the cards; it announces itself (a polite
 * live region) when [announce] says so ([ApprovalRules.announces]).
 */
@Composable
fun ApprovalCard(
    card: ApprovalView,
    decider: ApprovalDecider,
    modifier: Modifier = Modifier,
    voice: Boolean = false,
    announce: Boolean = false,
    position: Pair<Int, Int>? = null,
    argumentsMax: Dp = 160.dp,
) {
    LaunchedEffect(card.approvalRequestId, card.note) { decider.noted(card.approvalRequestId, card.note) }
    val tool = "${card.serverLabel}__${card.name}"
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer),
        modifier = modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(12.dp)) {
            Text(
                stringResource(R.string.approval_title, tool),
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.semantics {
                    heading()
                    if (announce) liveRegion = LiveRegionMode.Polite
                },
            )
            position?.takeIf { it.second > 1 }?.let { (i, n) ->
                Text(stringResource(R.string.approval_position, i, n), style = MaterialTheme.typography.labelSmall)
            }
            if (card.arguments.isNotBlank()) {
                val args = Modifier.padding(top = 4.dp)
                if (voice) {
                    Box(args.heightIn(max = argumentsMax).verticalScroll(rememberScrollState())) {
                        Text(card.arguments, style = MaterialTheme.typography.bodySmall)
                    }
                } else {
                    Text(card.arguments, style = MaterialTheme.typography.bodySmall, modifier = args)
                }
            }
            Text(stringResource(R.string.approval_where, card.threadTitle), style = MaterialTheme.typography.bodySmall)
            if (card.inCall) {
                Text(stringResource(R.string.approval_in_call), style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(top = 2.dp))
            }
            card.note?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 6.dp)) }
            decider.errors[card.approvalRequestId]?.let {
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 6.dp))
            }
            when (ApprovalRules.status(card)) {
                ApprovalRules.Status.ASK -> {
                    FlowRow(
                        Modifier.padding(top = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Button(onClick = { decider.decide(card.approvalRequestId, true) }) { Text(stringResource(R.string.approve)) }
                        OutlinedButton(onClick = { decider.decide(card.approvalRequestId, false) }) { Text(stringResource(R.string.decline)) }
                    }
                    Text(
                        stringResource(R.string.approval_moves_on),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
                ApprovalRules.Status.HELD -> Text(
                    pluralStringResource(
                        if (card.verdict == true) R.plurals.approval_held_approved else R.plurals.approval_held_declined,
                        card.waiting.toInt(),
                        card.waiting.toInt(),
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 8.dp),
                )
                ApprovalRules.Status.SENT -> Text(
                    stringResource(if (card.verdict == true) R.string.approval_sent_approved else R.string.approval_sent_declined),
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
        }
    }
}
