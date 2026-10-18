package dev.rootshell.kai.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.R

/*
 * Labelled controls (design §10, the accessibility audit): the label and its control are one
 * TalkBack item and one touch target ("Push-to-talk, switch, on"), never a bare control with its
 * label beside it, and the row is at least 48 dp high.
 */

/** A checkbox with its label; the whole row toggles it. */
@Composable
fun LabelledCheckbox(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).toggleable(value = checked, role = Role.Checkbox, onValueChange = onChange),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Text(label, modifier = Modifier.weight(1f))
    }
}

/** A switch after its label; the whole row toggles it. */
@Composable
fun LabelledSwitch(label: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).toggleable(value = on, role = Role.Switch, onValueChange = onChange),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = on, onCheckedChange = null)
    }
}

/** A radio button with its label; the whole row selects it. */
@Composable
fun LabelledRadio(label: String, selected: Boolean, onSelect: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(selected = selected, role = Role.RadioButton, onClick = onSelect),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = null)
        Text(label)
    }
}

/**
 * A setup step (design §9.5): a mark, its words and, while it is open, its action. TalkBack hears
 * the words with "done" or "to do", not the mark's glyph.
 */
@Composable
fun SetupStep(text: String, done: Boolean, action: String = stringResource(R.string.background_do), onDo: () -> Unit) {
    val state = stringResource(if (done) R.string.step_done else R.string.step_open)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(if (done) "✓" else "•", style = MaterialTheme.typography.titleMedium, modifier = Modifier.clearAndSetSemantics {})
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f).semantics { stateDescription = state },
        )
        if (!done) OutlinedButton(onClick = onDo) { Text(action) }
    }
}
